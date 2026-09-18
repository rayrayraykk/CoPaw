//! Original Gemini image and video capability checks over native REST.

use super::*;

pub(super) async fn probe(provider: &RemoteProvider, model: &str) -> RemoteProbe {
    let image = request(provider, model, false).await;
    let (supports_image, image_message) = evaluate_probe(image, "red", "Image");
    // The original Gemini provider probes video independently of image support.
    let video = request(provider, model, true).await;
    let (supports_video, video_message) = match video {
        Ok(answer) => {
            let answer = answer.trim().to_lowercase();
            if answer.contains("yes") {
                (true, format!("Video supported (answer={answer:?})"))
            } else {
                (
                    false,
                    format!("Model did not recognise video (answer={answer:?})"),
                )
            }
        }
        Err(error) => evaluate_probe(Err(error), "blue", "Video"),
    };
    RemoteProbe {
        supports_image,
        supports_video,
        image_message: redact(&image_message, provider),
        video_message: redact(&video_message, provider),
    }
}

async fn request(
    provider: &RemoteProvider,
    model: &str,
    video: bool,
) -> Result<String, RemoteFailure> {
    let endpoint = super::super::desktop_models::model_probe_path("GeminiChatModel", model)
        .map_err(|message| invalid_response(message, None))?;
    let url = protocol_endpoint(provider, &endpoint)?;
    let (media, prompt, tokens) = if video {
        (
            json!({"fileData": {"mimeType": "video/mp4", "fileUri": PROBE_VIDEO_URL}}),
            "Does this contain moving content? Reply with ONLY 'yes' or 'no', nothing else.",
            10,
        )
    } else {
        (
            json!({"inlineData": {"mimeType": "image/png", "data": PROBE_IMAGE_B64}}),
            "What is the single dominant color of this image? Reply with ONLY the color name, nothing else.",
            20,
        )
    };
    let response = send(
        provider,
        Method::POST,
        url,
        Some(json!({
            "contents": [{"role": "user", "parts": [media, {"text": prompt}]}],
            "generationConfig": {"maxOutputTokens": tokens}
        })),
        REQUEST_TIMEOUT_SECONDS,
    )
    .await?;
    if !response.status().is_success() {
        let mut error = response_failure(response, provider).await;
        error.message = redact(&error.message, provider);
        return Err(error);
    }
    let payload = read_json(response, provider.secret.as_deref()).await?;
    let candidate = payload.pointer("/candidates/0");
    if payload.get("error").is_some()
        || payload
            .pointer("/promptFeedback/blockReason")
            .and_then(Value::as_str)
            .is_some_and(|reason| reason != "BLOCK_REASON_UNSPECIFIED")
        || candidate
            .and_then(|candidate| candidate.get("finishReason"))
            .and_then(Value::as_str)
            .is_some_and(|reason| !matches!(reason, "STOP" | "MAX_TOKENS"))
    {
        return Err(invalid_response(
            "Gemini probe did not complete successfully",
            Some(200),
        ));
    }
    let text = candidate
        .and_then(|candidate| candidate.pointer("/content/parts"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| part.get("thought") != Some(&Value::Bool(true)))
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    if text.trim().is_empty() {
        return Err(invalid_response(
            "Gemini probe response did not contain model text",
            Some(200),
        ));
    }
    Ok(text)
}

fn redact(message: &str, provider: &RemoteProvider) -> String {
    let mut message = message.to_owned();
    for value in provider
        .secret
        .iter()
        .chain(provider.custom_headers.iter().map(|(_, value)| value))
    {
        message = redact_secret(message, Some(value));
        // Probe answers are lowercased for the original color/yes detection.
        message = redact_secret(message, Some(&value.to_lowercase()));
    }
    message
}

fn invalid_response(message: &str, status: Option<u16>) -> RemoteFailure {
    RemoteFailure {
        message: message.to_owned(),
        error_kind: "incompatible_api",
        http_status: status,
        retryable: false,
    }
}
