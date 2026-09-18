use pretty_assertions::assert_eq;

use super::*;

const PROBE_PATH: &str = "/api/models/gemini/models/fixture-gemini/probe-multimodal";

fn answer(text: &str) -> Value {
    json!({"candidates": [{"content": {"parts": [{"text": text}]}, "finishReason": "STOP"}]})
}

#[tokio::test]
async fn native_multimodal_probe_payloads_persist_and_update_both_capabilities() {
    let fixture = Fixture::new(false).await;
    fixture.configure().await;
    assert_eq!(
        api(&fixture.server, "POST", PROBE_PATH, json!({})).await,
        json!({
            "supports_image": true, "supports_video": true, "supports_multimodal": true,
            "image_message": "Image supported (answer=\"red\")", "video_message": "Video supported (answer=\"yes\")"
        })
    );
    {
        let remote = fixture.remote.lock().unwrap();
        assert_eq!(remote.probes.len(), 2);
        assert_eq!(
            remote.probes[0].1,
            json!({"contents": [{"role": "user", "parts": [
            {"inlineData": {"mimeType": "image/png", "data": concat!(
                "iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAIAAAD8GO2jAAAAJ0lEQVR42u3NsQkAAAjA",
                "sP7/tF7hIASyp6lTCQQCgUAgEAgEgi/BAjLD/C5w/SM9AAAAAElFTkSuQmCC")}},
            {"text": "What is the single dominant color of this image? Reply with ONLY the color name, nothing else."}]}],
            "generationConfig": {"maxOutputTokens": 20}})
        );
        assert_eq!(
            remote.probes[1].1,
            json!({"contents": [{"role": "user", "parts": [
            {"fileData": {"mimeType": "video/mp4", "fileUri": "https://help-static-aliyun-doc.aliyuncs.com/file-manage-files/zh-CN/20241115/cqqkru/1.mp4"}},
            {"text": "Does this contain moving content? Reply with ONLY 'yes' or 'no', nothing else."}]}],
            "generationConfig": {"maxOutputTokens": 10}})
        );
        for (headers, _) in &remote.probes {
            assert_eq!(headers["x-goog-api-key"], "gemini-fixture-private-key");
            assert_eq!(headers["x-fixture"], "native");
            assert!(!headers.contains_key("authorization"));
        }
    }
    assert_capabilities(&fixture, true, true);
    fixture
        .remote
        .lock()
        .unwrap()
        .media_replies
        .insert(String::from("video"), (StatusCode::OK, answer("no")));
    assert_eq!(
        api(&fixture.server, "POST", PROBE_PATH, json!({})).await,
        json!({
            "supports_image": true, "supports_video": false, "supports_multimodal": true,
            "image_message": "Image supported (answer=\"red\")", "video_message": "Model did not recognise video (answer=\"no\")"
        })
    );
    assert_capabilities(&fixture, true, false);
    fixture
        .remote
        .lock()
        .unwrap()
        .media_replies
        .insert(String::from("image"), (StatusCode::OK, answer("green")));
    assert_eq!(
        api(&fixture.server, "POST", PROBE_PATH, json!({})).await,
        json!({
            "supports_image": false, "supports_video": false, "supports_multimodal": false,
            "image_message": "Model did not recognise image (answer=\"green\")", "video_message": "Model did not recognise video (answer=\"no\")"
        })
    );
    assert_capabilities(&fixture, false, false);
}

fn assert_capabilities(fixture: &Fixture, image: bool, video: bool) {
    let registry = read_registry_from(desktop_workspace(&fixture.server).unwrap()).unwrap();
    let model = registry.providers["gemini"]
        .extra_models
        .iter()
        .find(|model| model.id == "fixture-gemini")
        .unwrap();
    assert_eq!(
        (
            model.supports_image,
            model.supports_video,
            model.supports_multimodal,
            model.probe_source.as_deref()
        ),
        (
            Some(image),
            Some(video),
            Some(image || video),
            Some("probed")
        )
    );
}

#[tokio::test]
async fn image_failures_do_not_skip_video_or_leak_secrets_or_retry_bad_requests() {
    let fixture = Fixture::new(false).await;
    fixture.configure().await;
    api(
        &fixture.server,
        "PUT",
        "/api/models/gemini/config",
        json!({"custom_headers": {"x-fixture": "Private-Header"}}),
    )
    .await;
    for (status, payload, diagnostic) in [
        (
            StatusCode::BAD_REQUEST,
            json!({"error": {"message": "image rejected gemini-fixture-private-key Private-Header"}}),
            "not supported",
        ),
        (
            StatusCode::UNAUTHORIZED,
            json!({"error": {"message": "gemini-fixture-private-key Private-Header"}}),
            "inconclusive",
        ),
        (
            StatusCode::OK,
            json!({"promptFeedback": {"blockReason": "SAFETY"}, "candidates": [{"content": {"parts": [{"text": "red"}]}}]}),
            "did not complete",
        ),
        (
            StatusCode::OK,
            json!({"candidates": [{"content": {"parts": [{"text": "red", "thought": true}]}, "finishReason": "STOP"}]}),
            "did not contain model text",
        ),
        (
            StatusCode::OK,
            json!({"candidates": [{"content": {"parts": [{"text": "red"}]}, "finishReason": "SAFETY"}]}),
            "did not complete",
        ),
        (StatusCode::OK, json!("invalid json"), "JSON"),
        (
            StatusCode::OK,
            answer("Private-Header"),
            "did not recognise",
        ),
    ] {
        {
            let mut remote = fixture.remote.lock().unwrap();
            remote.probes.clear();
            remote
                .media_replies
                .insert(String::from("image"), (status, payload));
        }
        let result = api(&fixture.server, "POST", PROBE_PATH, json!({})).await;
        let mut expected = json!({"supports_image": false, "supports_video": true, "supports_multimodal": true,
            "video_message": "Video supported (answer=\"yes\")"});
        expected["image_message"] = result["image_message"].clone();
        assert_eq!(result, expected);
        assert!(
            result["image_message"]
                .as_str()
                .unwrap()
                .contains(diagnostic),
            "{result:#}"
        );
        let encoded = result.to_string().to_lowercase();
        for secret in ["gemini-fixture-private-key", "private-header"] {
            assert!(!encoded.contains(secret));
        }
        assert_eq!(fixture.remote.lock().unwrap().probes.len(), 2);
        assert_capabilities(&fixture, false, true);
    }
    // HTTP 400 video errors are final for Gemini, not retried as OpenAI video formats.
    {
        let mut remote = fixture.remote.lock().unwrap();
        remote.probes.clear();
        remote.media_replies = BTreeMap::from([(
            String::from("video"),
            (
                StatusCode::BAD_REQUEST,
                json!({"error": "video unavailable"}),
            ),
        )]);
    }
    let result = api(&fixture.server, "POST", PROBE_PATH, json!({})).await;
    assert_eq!(result["supports_image"], true);
    assert_eq!(result["supports_video"], false);
    assert!(
        result["video_message"]
            .as_str()
            .unwrap()
            .contains("not supported")
    );
    assert_eq!(fixture.remote.lock().unwrap().probes.len(), 2);
}
