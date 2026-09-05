//! Bounded remote model discovery and capability probes.

use std::collections::BTreeSet;
use std::time::Duration;

use futures_util::StreamExt as _;
use reqwest::Method;
use reqwest::RequestBuilder;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;

const REQUEST_TIMEOUT_SECONDS: u64 = 15;
const VIDEO_HTTP_TIMEOUT_SECONDS: u64 = 45;
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_DISCOVERY_PAGES: usize = 32;
const MAX_DISCOVERED_MODELS: usize = 4_096;
const PROBE_IMAGE_B64: &str = concat!(
    "iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAIAAAD8GO2jAAAAJ0lEQVR42u3NsQkAAAjA",
    "sP7/tF7hIASyp6lTCQQCgUAgEAgEgi/BAjLD/C5w/SM9AAAAAElFTkSuQmCC"
);
const PROBE_VIDEO_B64: &str = concat!(
    "AAAAIGZ0eXBpc29tAAACAGlzb21pc28yYXZjMW1wNDEAAAAIZnJlZQAAA2Vt",
    "ZGF0AAACrgYF//+q3EXpvebZSLeWLNgg2SPu73gyNjQgLSBjb3JlIDE2NCBy",
    "MzEwOCAzMWUxOWY5IC0gSC4yNjQvTVBFRy00IEFWQyBjb2RlYyAtIENvcHls",
    "ZWZ0IDIwMDMtMjAyMyAtIGh0dHA6Ly93d3cudmlkZW9sYW4ub3JnL3gyNjQu",
    "aHRtbCAtIG9wdGlvbnM6IGNhYmFjPTEgcmVmPTMgZGVibG9jaz0xOjA6MCBh",
    "bmFseXNlPTB4MzoweDExMyBtZT1oZXggc3VibWU9NyBwc3k9MSBwc3lfcmQ9",
    "MS4wMDowLjAwIG1peGVkX3JlZj0xIG1lX3JhbmdlPTE2IGNocm9tYV9tZT0x",
    "IHRyZWxsaXM9MSA4eDhkY3Q9MSBjcW09MCBkZWFkem9uZT0yMSwxMSBmYXN0",
    "X3Bza2lwPTEgY2hyb21hX3FwX29mZnNldD0tMiB0aHJlYWRzPTIgbG9va2Fo",
    "ZWFkX3RocmVhZHM9MSBzbGljZWRfdGhyZWFkcz0wIG5yPTAgZGVjaW1hdGU9",
    "MSBpbnRlcmxhY2VkPTAgYmx1cmF5X2NvbXBhdD0wIGNvbnN0cmFpbmVkX2lu",
    "dHJhPTAgYmZyYW1lcz0zIGJfcHlyYW1pZD0yIGJfYWRhcHQ9MSBiX2JpYXM9",
    "MCBkaXJlY3Q9MSB3ZWlnaHRiPTEgb3Blbl9nb3A9MCB3ZWlnaHRwPTIga2V5",
    "aW50PTI1MCBrZXlpbnRfbWluPTEwIHNjZW5lY3V0PTQwIGludHJhX3JlZnJl",
    "c2g9MCByY19sb29rYWhlYWQ9NDAgcmM9Y3JmIG1idHJlZT0xIGNyZj0yMy4w",
    "IHFjb21wPTAuNjAgcXBtaW49MCBxcG1heD02OSBxcHN0ZXA9NCBpcF9yYXRp",
    "bz0xLjQwIGFxPTE6MS4wMACAAAAAJ2WIhAAR//7n4/wKbYEB8Tpk2PtANbXc",
    "qLo1x7YozakvH3bhD2xGfwAAAApBmiRsQQ/+qlfeAAAACEGeQniHfwW9AAAA",
    "CAGeYXRDfwd8AAAACAGeY2pDfwd9AAAAEEGaaEmoQWiZTAh3//6pnTUAAAAK",
    "QZ6GRREsO/8FvQAAAAgBnqV0Q38HfQAAAAgBnqdqQ38HfAAAABBBmqlJqEFs",
    "mUwIb//+p4+IAAADoG1vb3YAAABsbXZoZAAAAAAAAAAAAAAAAAAAA+gAAAPo",
    "AAEAAAEAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAA",
    "AAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAALLdHJhawAA",
    "AFx0a2hkAAAAAwAAAAAAAAAAAAAAAQAAAAAAAAPoAAAAAAAAAAAAAAAAAAAA",
    "AAABAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAQAAAAABAAAAAQAAA",
    "AAAAJGVkdHMAAAAcZWxzdAAAAAAAAAABAAAD6AAACAAAAQAAAAACQ21kaWEA",
    "AAAgbWRoZAAAAAAAAAAAAAAAAAAAKAAAACgAVcQAAAAAAC1oZGxyAAAAAAAA",
    "AAB2aWRlAAAAAAAAAAAAAAAAVmlkZW9IYW5kbGVyAAAAAe5taW5mAAAAFHZt",
    "aGQAAAABAAAAAAAAAAAAAAAkZGluZgAAABxkcmVmAAAAAAAAAAEAAAAMdXJs",
    "IAAAAAEAAAGuc3RibAAAAK5zdHNkAAAAAAAAAAEAAACeYXZjMQAAAAAAAAAB",
    "AAAAAAAAAAAAAAAAAAAAAABAAEAASAAAAEgAAAAAAAAAAQAAAAAAAAAAAAAA",
    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAABj//wAAADRhdmNDAWQACv/hABdnZAAK",
    "rNlEJoQAAAMABAAAAwBQPEiWWAEABmjr48siwP34+AAAAAAUYnRydAAAAAAA",
    "AE4gAAAa6AAAABhzdHRzAAAAAAAAAAEAAAAKAAAEAAAAABRzdHNzAAAAAAAA",
    "AAEAAAABAAAAYGN0dHMAAAAAAAAACgAAAAEAAAgAAAAAAQAAFAAAAAABAAAI",
    "AAAAAAEAAAAAAAAAAQAABAAAAAABAAAUAAAAAAEAAAgAAAAAAQAAAAAAAAAB",
    "AAAEAAAAAAEAAAgAAAAAHHN0c2MAAAAAAAAAAQAAAAEAAAAKAAAAAQAAADxz",
    "dHN6AAAAAAAAAAAAAAAKAAAC3QAAAA4AAAAMAAAADAAAAAwAAAAUAAAADgAA",
    "AAwAAAAMAAAAFAAAABRzdGNvAAAAAAAAAAEAAAAwAAAAYXVkdGEAAABZbWV0",
    "YQAAAAAAAAAhaGRscgAAAAAAAAAAbWRpcmFwcGwAAAAAAAAAAAAAAAAsaWxz",
    "dAAAACSpdG9vAAAAHGRhdGEAAAABAAAAAExhdmY2MS43LjEwMA=="
);
const PROBE_VIDEO_URL: &str = concat!(
    "https://help-static-aliyun-doc.aliyuncs.com",
    "/file-manage-files/zh-CN/20241115/cqqkru/1.mp4"
);

#[derive(Debug, Clone)]
pub(super) struct RemoteProvider {
    pub base_url: String,
    pub chat_model: String,
    pub custom_headers: Vec<(String, String)>,
    pub auth_mode: String,
    pub secret: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiscoveredModel {
    pub id: String,
    pub name: String,
    pub max_input_length: Option<u64>,
    pub max_output_length: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteFailure {
    pub message: String,
    pub error_kind: &'static str,
    pub http_status: Option<u16>,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteConnection {
    pub success: bool,
    pub message: String,
    pub status: &'static str,
    pub http_status: Option<u16>,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RemoteProbe {
    pub supports_image: bool,
    pub supports_video: bool,
    pub image_message: String,
    pub video_message: String,
}

pub(super) async fn test_provider(
    provider: &RemoteProvider,
    fallback_model: &str,
) -> RemoteConnection {
    let response = send(
        provider,
        Method::GET,
        match protocol_endpoint(provider, "models") {
            Ok(url) => url,
            Err(error) => return connection_from_failure(&error),
        },
        None,
        REQUEST_TIMEOUT_SECONDS,
    )
    .await;
    match response {
        Ok(response) if response.status().is_success() => RemoteConnection {
            success: true,
            message: String::from("Connection successful"),
            status: "available",
            http_status: Some(response.status().as_u16()),
            retryable: false,
        },
        Ok(response)
            if provider.chat_model == "AnthropicChatModel"
                && matches!(response.status().as_u16(), 404 | 405) =>
        {
            test_anthropic_messages(provider, fallback_model).await
        }
        Ok(response) => connection_from_failure(&response_failure(response, provider).await),
        Err(error) => connection_from_failure(&error),
    }
}

pub(super) async fn discover_models(
    provider: &RemoteProvider,
) -> Result<Vec<DiscoveredModel>, RemoteFailure> {
    let mut url = protocol_endpoint(provider, "models")?;
    let mut models = Vec::new();
    let mut seen_ids = BTreeSet::new();
    let mut seen_cursors = BTreeSet::new();
    for _ in 0..MAX_DISCOVERY_PAGES {
        let response = send(
            provider,
            Method::GET,
            url.clone(),
            None,
            REQUEST_TIMEOUT_SECONDS,
        )
        .await?;
        if !response.status().is_success() {
            return Err(response_failure(response, provider).await);
        }
        let payload = read_json(response, provider.secret.as_deref()).await?;
        let rows = payload
            .get("data")
            .and_then(Value::as_array)
            .or_else(|| payload.as_array())
            .ok_or_else(|| RemoteFailure {
                message: String::from("Provider model catalog did not contain a data array"),
                error_kind: "incompatible_api",
                http_status: Some(200),
                retryable: false,
            })?;
        for row in rows {
            if let Some(model) = normalize_model(row)
                && seen_ids.insert(model.id.clone())
            {
                if models.len() >= MAX_DISCOVERED_MODELS {
                    return Err(RemoteFailure {
                        message: String::from("Provider model catalog exceeds 4096 models"),
                        error_kind: "incompatible_api",
                        http_status: Some(200),
                        retryable: false,
                    });
                }
                models.push(model);
            }
        }
        if !payload
            .get("has_more")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(models);
        }
        let cursor = payload
            .get("last_id")
            .and_then(Value::as_str)
            .or_else(|| {
                rows.last()
                    .and_then(|row| row.get("id"))
                    .and_then(Value::as_str)
            })
            .filter(|cursor| !cursor.is_empty())
            .ok_or_else(|| RemoteFailure {
                message: String::from("Provider model pagination omitted its cursor"),
                error_kind: "incompatible_api",
                http_status: Some(200),
                retryable: false,
            })?;
        if !seen_cursors.insert(cursor.to_owned()) {
            return Err(RemoteFailure {
                message: String::from("Provider model pagination repeated its cursor"),
                error_kind: "incompatible_api",
                http_status: Some(200),
                retryable: false,
            });
        }
        url.query_pairs_mut().append_pair("after", cursor);
    }
    Err(RemoteFailure {
        message: String::from("Provider model pagination exceeds 32 pages"),
        error_kind: "incompatible_api",
        http_status: Some(200),
        retryable: false,
    })
}

pub(super) async fn probe_multimodal(provider: &RemoteProvider, model_id: &str) -> RemoteProbe {
    let image = probe_media(provider, model_id, ProbeKind::Image, false).await;
    let (image_supported, image_message) = evaluate_probe(image, "red", "Image");
    if !image_supported {
        return RemoteProbe {
            supports_image: false,
            supports_video: false,
            image_message,
            video_message: String::from("Skipped: image probe failed"),
        };
    }
    let first_video = probe_media(provider, model_id, ProbeKind::VideoData, false).await;
    let video = match &first_video {
        Err(error) if error.http_status == Some(400) => {
            probe_media(provider, model_id, ProbeKind::VideoUrl, true).await
        }
        _ => first_video,
    };
    let (video_supported, video_message) = evaluate_probe(video, "blue", "Video");
    RemoteProbe {
        supports_image: true,
        supports_video: video_supported,
        image_message,
        video_message,
    }
}

async fn test_anthropic_messages(
    provider: &RemoteProvider,
    fallback_model: &str,
) -> RemoteConnection {
    let endpoint = match protocol_endpoint(provider, "messages") {
        Ok(endpoint) => endpoint,
        Err(error) => return connection_from_failure(&error),
    };
    let model = if fallback_model.trim().is_empty() {
        "claude-opus-4-5"
    } else {
        fallback_model
    };
    let response = send(
        provider,
        Method::POST,
        endpoint,
        Some(json!({
            "model": model,
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "ping"}]
        })),
        REQUEST_TIMEOUT_SECONDS,
    )
    .await;
    match response {
        Ok(response)
            if response.status().is_success()
                || matches!(response.status().as_u16(), 400 | 404 | 422) =>
        {
            RemoteConnection {
                success: true,
                message: String::from("Connection successful"),
                status: "available",
                http_status: Some(response.status().as_u16()),
                retryable: false,
            }
        }
        Ok(response) => connection_from_failure(&response_failure(response, provider).await),
        Err(error) => connection_from_failure(&error),
    }
}

#[derive(Debug, Clone, Copy)]
enum ProbeKind {
    Image,
    VideoData,
    VideoUrl,
}

async fn probe_media(
    provider: &RemoteProvider,
    model_id: &str,
    kind: ProbeKind,
    extended_timeout: bool,
) -> Result<String, RemoteFailure> {
    let endpoint = match provider.chat_model.as_str() {
        "OpenAIResponseModel" => "responses",
        "OpenAIChatModel" => "chat/completions",
        "AnthropicChatModel" => "messages",
        _ => {
            return Err(RemoteFailure {
                message: String::from("Unsupported provider protocol"),
                error_kind: "incompatible_api",
                http_status: None,
                retryable: false,
            });
        }
    };
    let url = protocol_endpoint(provider, endpoint)?;
    let body = probe_body(&provider.chat_model, model_id, kind);
    let response = send(
        provider,
        Method::POST,
        url,
        Some(body),
        if extended_timeout {
            VIDEO_HTTP_TIMEOUT_SECONDS
        } else {
            REQUEST_TIMEOUT_SECONDS
        },
    )
    .await?;
    if !response.status().is_success() {
        return Err(response_failure(response, provider).await);
    }
    let payload = read_json(response, provider.secret.as_deref()).await?;
    extract_answer(&provider.chat_model, &payload).ok_or_else(|| RemoteFailure {
        message: String::from("Provider probe response did not contain model text"),
        error_kind: "incompatible_api",
        http_status: Some(200),
        retryable: false,
    })
}

fn probe_body(chat_model: &str, model_id: &str, kind: ProbeKind) -> Value {
    let (openai_type, anthropic_type, media_url, anthropic_source, prompt) = match kind {
        ProbeKind::Image => (
            "image_url",
            "image",
            format!("data:image/png;base64,{PROBE_IMAGE_B64}"),
            json!({"type": "base64", "media_type": "image/png", "data": PROBE_IMAGE_B64}),
            "What is the single dominant color of this image? Reply with ONLY the color name, nothing else.",
        ),
        ProbeKind::VideoData => (
            "video_url",
            "video",
            format!("data:video/mp4;base64,{PROBE_VIDEO_B64}"),
            json!({"type": "base64", "media_type": "video/mp4", "data": PROBE_VIDEO_B64}),
            "What is the single dominant color shown in this video? Reply with ONLY the color name, nothing else.",
        ),
        ProbeKind::VideoUrl => (
            "video_url",
            "video",
            String::from(PROBE_VIDEO_URL),
            json!({"type": "url", "url": PROBE_VIDEO_URL}),
            "What is the single dominant color shown in this video? Reply with ONLY the color name, nothing else.",
        ),
    };
    let mut responses_media = Map::new();
    responses_media.insert(
        String::from("type"),
        Value::String(String::from(if matches!(kind, ProbeKind::Image) {
            "input_image"
        } else {
            "input_video"
        })),
    );
    responses_media.insert(
        String::from(if matches!(kind, ProbeKind::Image) {
            "image_url"
        } else {
            "video_url"
        }),
        Value::String(media_url.clone()),
    );
    let mut chat_media = Map::new();
    chat_media.insert(
        String::from("type"),
        Value::String(String::from(openai_type)),
    );
    chat_media.insert(String::from(openai_type), json!({"url": media_url}));
    match chat_model {
        "OpenAIResponseModel" => json!({
            "model": model_id,
            "input": [{"role": "user", "content": [
                Value::Object(responses_media),
                {"type": "input_text", "text": prompt}
            ]}],
            "max_output_tokens": 1024
        }),
        "AnthropicChatModel" => json!({
            "model": model_id,
            "max_tokens": 200,
            "messages": [{"role": "user", "content": [
                {"type": anthropic_type, "source": anthropic_source},
                {"type": "text", "text": prompt}
            ]}]
        }),
        _ => json!({
            "model": model_id,
            "max_tokens": 200,
            "messages": [{"role": "user", "content": [
                Value::Object(chat_media),
                {"type": "text", "text": prompt}
            ]}]
        }),
    }
}

fn evaluate_probe(
    response: Result<String, RemoteFailure>,
    expected_color: &str,
    label: &str,
) -> (bool, String) {
    match response {
        Ok(answer) => {
            let answer = answer.trim().to_lowercase();
            let matched = if expected_color == "red" {
                ["red", "scarlet", "crimson", "vermilion", "maroon", "红"]
                    .iter()
                    .any(|color| answer.contains(color))
            } else {
                ["blue", "navy", "azure", "cobalt", "cyan", "indigo", "蓝"]
                    .iter()
                    .any(|color| answer.contains(color))
            };
            if matched {
                (true, format!("{label} supported (answer={answer:?})"))
            } else {
                (
                    false,
                    format!(
                        "Model did not recognise {} (answer={answer:?})",
                        label.to_lowercase()
                    ),
                )
            }
        }
        Err(error) => {
            let kind = if error.http_status == Some(400) || contains_media_marker(&error.message) {
                "not supported"
            } else {
                "probe inconclusive"
            };
            (false, format!("{label} {kind}: {}", error.message))
        }
    }
}

fn extract_answer(chat_model: &str, payload: &Value) -> Option<String> {
    if chat_model == "OpenAIResponseModel" {
        if let Some(text) = payload.get("output_text").and_then(Value::as_str) {
            return Some(text.to_owned());
        }
        return payload
            .get("output")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .flat_map(|item| {
                item.get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .find_map(|part| part.get("text").and_then(Value::as_str))
            .map(str::to_owned);
    }
    if chat_model == "AnthropicChatModel" {
        let text = payload
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<String>();
        return (!text.is_empty()).then_some(text);
    }
    let content = payload.pointer("/choices/0/message/content")?;
    if let Some(text) = content.as_str() {
        return Some(text.to_owned());
    }
    let text = content
        .as_array()?
        .iter()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn normalize_model(row: &Value) -> Option<DiscoveredModel> {
    let id = row.get("id")?.as_str()?.trim();
    if id.is_empty() || id.len() > 1_024 || id.chars().any(char::is_control) {
        return None;
    }
    let name = row
        .get("name")
        .or_else(|| row.get("display_name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.len() <= 1_024)
        .unwrap_or(id);
    let max_input_length = ["context_length", "max_model_len", "max_context_length"]
        .iter()
        .find_map(|field| row.get(field).and_then(number_as_u64))
        .filter(|value| *value >= 1_000);
    let max_output_length = row
        .get("max_output_tokens")
        .and_then(number_as_u64)
        .filter(|value| *value > 0);
    Some(DiscoveredModel {
        id: id.to_owned(),
        name: name.to_owned(),
        max_input_length,
        max_output_length,
    })
}

fn number_as_u64(value: &Value) -> Option<u64> {
    value.as_u64()
}

async fn send(
    provider: &RemoteProvider,
    method: Method,
    url: url::Url,
    body: Option<Value>,
    timeout_seconds: u64,
) -> Result<reqwest::Response, RemoteFailure> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(timeout_seconds))
        .build()
        .map_err(request_failure)?;
    let mut request = client.request(method, url);
    request = apply_headers(request, provider);
    if let Some(body) = body {
        request = request.json(&body);
    }
    request.send().await.map_err(request_failure)
}

fn apply_headers(mut request: RequestBuilder, provider: &RemoteProvider) -> RequestBuilder {
    for (name, value) in &provider.custom_headers {
        request = request.header(name, value);
    }
    if provider.chat_model == "AnthropicChatModel" {
        request = request.header("anthropic-version", "2023-06-01");
    }
    if let Some(secret) = provider
        .secret
        .as_deref()
        .filter(|secret| !secret.is_empty())
    {
        if provider.chat_model == "AnthropicChatModel" && provider.auth_mode == "api_key" {
            request = request.header("x-api-key", secret);
        } else {
            request = request.bearer_auth(secret);
        }
    }
    request
}

fn protocol_endpoint(provider: &RemoteProvider, endpoint: &str) -> Result<url::Url, RemoteFailure> {
    let base = provider.base_url.trim();
    if base.is_empty() {
        return Err(RemoteFailure {
            message: String::from("Provider Base URL is empty"),
            error_kind: "incompatible_api",
            http_status: None,
            retryable: false,
        });
    }
    let path = if provider.chat_model == "AnthropicChatModel"
        && !url::Url::parse(base).is_ok_and(|url| url.path().trim_end_matches('/').ends_with("/v1"))
    {
        format!("v1/{endpoint}")
    } else {
        endpoint.to_owned()
    };
    url::Url::parse(&format!("{}/{path}", base.trim_end_matches('/'))).map_err(|error| {
        RemoteFailure {
            message: format!("Provider Base URL is invalid: {error}"),
            error_kind: "incompatible_api",
            http_status: None,
            retryable: false,
        }
    })
}

async fn response_failure(response: reqwest::Response, provider: &RemoteProvider) -> RemoteFailure {
    let status = response.status().as_u16();
    let detail = match read_limited(response).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).trim().to_owned(),
        Err(error) => error.message,
    };
    let detail = redact_secret(
        if detail.is_empty() {
            String::from("Provider returned an empty error response")
        } else {
            detail
        },
        provider.secret.as_deref(),
    );
    let (error_kind, retryable) = classify_failure(status, &detail);
    RemoteFailure {
        message: format!("HTTP {status}: {detail}"),
        error_kind,
        http_status: Some(status),
        retryable,
    }
}

async fn read_json(
    response: reqwest::Response,
    secret: Option<&str>,
) -> Result<Value, RemoteFailure> {
    let status = response.status().as_u16();
    let bytes = read_limited(response).await?;
    serde_json::from_slice(&bytes).map_err(|error| RemoteFailure {
        message: redact_secret(format!("Provider returned invalid JSON: {error}"), secret),
        error_kind: "incompatible_api",
        http_status: Some(status),
        retryable: false,
    })
}

async fn read_limited(response: reqwest::Response) -> Result<Vec<u8>, RemoteFailure> {
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(request_failure)?;
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(RemoteFailure {
                message: String::from("Provider response exceeds 2 MiB"),
                error_kind: "incompatible_api",
                http_status: None,
                retryable: false,
            });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn request_failure(error: impl std::fmt::Display) -> RemoteFailure {
    RemoteFailure {
        message: format!("Provider request failed: {error}"),
        error_kind: "transient_error",
        http_status: None,
        retryable: true,
    }
}

fn connection_from_failure(error: &RemoteFailure) -> RemoteConnection {
    RemoteConnection {
        success: false,
        message: format!("Connection failed: {}", error.message),
        status: error.error_kind,
        http_status: error.http_status,
        retryable: error.retryable,
    }
}

fn classify_failure(status: u16, detail: &str) -> (&'static str, bool) {
    let detail = detail.to_ascii_lowercase();
    if matches!(status, 401 | 403)
        || [
            "unauthorized",
            "forbidden",
            "invalid api key",
            "authentication",
        ]
        .iter()
        .any(|marker| detail.contains(marker))
    {
        ("permission_denied", false)
    } else if status == 404 || detail.contains("not found") {
        ("model_not_found", false)
    } else if status == 429 || detail.contains("rate limit") {
        ("rate_limited", true)
    } else if status >= 500 {
        ("transient_error", true)
    } else {
        ("incompatible_api", false)
    }
}

fn contains_media_marker(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    [
        "image",
        "video",
        "vision",
        "multimodal",
        "image_url",
        "video_url",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

fn redact_secret(mut message: String, secret: Option<&str>) -> String {
    if let Some(secret) = secret.filter(|secret| !secret.is_empty()) {
        message = message.replace(secret, "[REDACTED]");
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mock_models(
        axum::extract::Path(protocol): axum::extract::Path<String>,
        axum::extract::Query(query): axum::extract::Query<
            std::collections::BTreeMap<String, String>,
        >,
        headers: axum::http::HeaderMap,
    ) -> (axum::http::StatusCode, axum::Json<Value>) {
        if !mock_authorized(&protocol, &headers) {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                axum::Json(json!({"error": "invalid key"})),
            );
        }
        if query.contains_key("after") {
            return (
                axum::http::StatusCode::OK,
                axum::Json(json!({
                    "data": [
                        {"id": format!("{protocol}-one")},
                        {
                            "id": format!("{protocol}-two"),
                            "display_name": "Second Model",
                            "max_model_len": 64000
                        }
                    ],
                    "has_more": false
                })),
            );
        }
        (
            axum::http::StatusCode::OK,
            axum::Json(json!({
                "data": [{
                    "id": format!("{protocol}-one"),
                    "name": "First Model",
                    "context_length": 262_144,
                    "max_output_tokens": 8192
                }],
                "has_more": true,
                "last_id": format!("{protocol}-one")
            })),
        )
    }

    async fn mock_probe(
        axum::extract::Path(protocol): axum::extract::Path<String>,
        headers: axum::http::HeaderMap,
        axum::Json(body): axum::Json<Value>,
    ) -> (axum::http::StatusCode, axum::Json<Value>) {
        if !mock_authorized(&protocol, &headers) {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                axum::Json(json!({"error": "invalid key"})),
            );
        }
        let encoded = body.to_string();
        let answer = if encoded.contains("image") {
            "red"
        } else {
            "blue"
        };
        let response = match protocol.as_str() {
            "responses" => json!({"output_text": answer}),
            "anthropic" => json!({"content": [{"type": "text", "text": answer}]}),
            _ => json!({"choices": [{"message": {"content": answer}}]}),
        };
        (axum::http::StatusCode::OK, axum::Json(response))
    }

    fn mock_authorized(protocol: &str, headers: &axum::http::HeaderMap) -> bool {
        let tenant = headers
            .get("x-tenant")
            .and_then(|value| value.to_str().ok());
        let credential = if protocol == "anthropic" {
            headers.get("x-api-key")
        } else {
            headers.get(axum::http::header::AUTHORIZATION)
        }
        .and_then(|value| value.to_str().ok());
        tenant == Some("team-a")
            && credential
                == Some(if protocol == "anthropic" {
                    "test-key"
                } else {
                    "Bearer test-key"
                })
    }

    async fn mock_redirect() -> impl axum::response::IntoResponse {
        (
            axum::http::StatusCode::TEMPORARY_REDIRECT,
            [(axum::http::header::LOCATION, "/openai/v1/models")],
        )
    }

    async fn mock_oversized() -> String {
        "x".repeat(MAX_RESPONSE_BYTES + 1)
    }

    async fn mock_invalid_json() -> &'static str {
        "this is not JSON"
    }

    async fn mock_unauthorized() -> (axum::http::StatusCode, &'static str) {
        (
            axum::http::StatusCode::UNAUTHORIZED,
            "credential test-key was rejected",
        )
    }

    #[test]
    fn normalizes_model_catalog_rows_and_endpoints() {
        assert_eq!(
            normalize_model(&json!({
                "id": "vendor/model",
                "display_name": "Vendor Model",
                "context_length": 131_072,
                "max_output_tokens": 8192
            })),
            Some(DiscoveredModel {
                id: String::from("vendor/model"),
                name: String::from("Vendor Model"),
                max_input_length: Some(131_072),
                max_output_length: Some(8_192),
            })
        );
        let anthropic = RemoteProvider {
            base_url: String::from("https://example.test/anthropic"),
            chat_model: String::from("AnthropicChatModel"),
            custom_headers: Vec::new(),
            auth_mode: String::from("api_key"),
            secret: None,
        };
        assert_eq!(
            protocol_endpoint(&anthropic, "models")
                .expect("Anthropic model endpoint should resolve")
                .as_str(),
            "https://example.test/anthropic/v1/models"
        );
    }

    #[test]
    fn evaluates_probe_answers_semantically() {
        assert_eq!(
            evaluate_probe(Ok(String::from("Crimson")), "red", "Image"),
            (true, String::from("Image supported (answer=\"crimson\")"))
        );
        assert!(!evaluate_probe(Ok(String::from("unknown")), "blue", "Video").0);
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn tests_discovers_and_probes_all_remote_protocols() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("remote model mock should bind");
        let address = listener
            .local_addr()
            .expect("remote model mock should have an address");
        let app = axum::Router::new()
            .route("/{protocol}/v1/models", axum::routing::get(mock_models))
            .route(
                "/{protocol}/v1/chat/completions",
                axum::routing::post(mock_probe),
            )
            .route("/{protocol}/v1/responses", axum::routing::post(mock_probe))
            .route("/{protocol}/v1/messages", axum::routing::post(mock_probe))
            .route("/redirect/v1/models", axum::routing::get(mock_redirect))
            .route("/oversized/v1/models", axum::routing::get(mock_oversized))
            .route("/invalid/v1/models", axum::routing::get(mock_invalid_json))
            .route(
                "/unauthorized/v1/models",
                axum::routing::get(mock_unauthorized),
            );
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("remote model mock should run");
        });

        for (protocol, chat_model) in [
            ("openai", "OpenAIChatModel"),
            ("responses", "OpenAIResponseModel"),
            ("anthropic", "AnthropicChatModel"),
        ] {
            let provider = RemoteProvider {
                base_url: format!("http://{address}/{protocol}/v1"),
                chat_model: String::from(chat_model),
                custom_headers: vec![(String::from("x-tenant"), String::from("team-a"))],
                auth_mode: String::from("api_key"),
                secret: Some(String::from("test-key")),
            };
            let connection = test_provider(&provider, &format!("{protocol}-one")).await;
            assert_eq!(
                connection,
                RemoteConnection {
                    success: true,
                    message: String::from("Connection successful"),
                    status: "available",
                    http_status: Some(200),
                    retryable: false,
                }
            );
            let models = discover_models(&provider)
                .await
                .expect("remote model discovery should succeed");
            assert_eq!(
                models,
                vec![
                    DiscoveredModel {
                        id: format!("{protocol}-one"),
                        name: String::from("First Model"),
                        max_input_length: Some(262_144),
                        max_output_length: Some(8_192),
                    },
                    DiscoveredModel {
                        id: format!("{protocol}-two"),
                        name: String::from("Second Model"),
                        max_input_length: Some(64_000),
                        max_output_length: None,
                    },
                ]
            );
            assert_eq!(
                probe_multimodal(&provider, &format!("{protocol}-one")).await,
                RemoteProbe {
                    supports_image: true,
                    supports_video: true,
                    image_message: String::from("Image supported (answer=\"red\")"),
                    video_message: String::from("Video supported (answer=\"blue\")"),
                }
            );
        }
        let failure_provider = |protocol: &str| RemoteProvider {
            base_url: format!("http://{address}/{protocol}/v1"),
            chat_model: String::from("OpenAIChatModel"),
            custom_headers: Vec::new(),
            auth_mode: String::from("api_key"),
            secret: Some(String::from("test-key")),
        };
        let redirected = test_provider(&failure_provider("redirect"), "model").await;
        assert!(!redirected.success);
        assert_eq!(redirected.http_status, Some(307));
        let oversized = discover_models(&failure_provider("oversized"))
            .await
            .expect_err("oversized discovery should fail");
        assert_eq!(oversized.message, "Provider response exceeds 2 MiB");
        let invalid = discover_models(&failure_provider("invalid"))
            .await
            .expect_err("invalid discovery JSON should fail");
        assert!(
            invalid
                .message
                .starts_with("Provider returned invalid JSON:")
        );
        let unauthorized = test_provider(&failure_provider("unauthorized"), "model").await;
        assert!(!unauthorized.success);
        assert_eq!(unauthorized.status, "permission_denied");
        assert!(!unauthorized.message.contains("test-key"));
        assert!(unauthorized.message.contains("[REDACTED]"));
        task.abort();
    }
}
