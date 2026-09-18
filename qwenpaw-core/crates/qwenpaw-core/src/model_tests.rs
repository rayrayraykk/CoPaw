use std::convert::Infallible;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::Response;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::routing::post;
use bytes::Bytes;
use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use qwenpaw_storage::StoredMessage;

use super::*;

#[test]
fn decodes_fragmented_crlf_sse_and_skips_comments() {
    let mut decoder = SseDecoder::default();
    decoder
        .push(b": keepalive\r\n\r\ndata: {\"choices\":[]}")
        .expect("first fragment should decode");
    assert_eq!(decoder.next_data().expect("comment should parse"), None);
    decoder
        .push(b"\r\n\r\ndata: [DO")
        .expect("second fragment should decode");
    assert_eq!(
        decoder.next_data().expect("JSON event should parse"),
        Some(String::from("{\"choices\":[]}"))
    );
    decoder
        .push(b"NE]\n\n")
        .expect("final fragment should decode");
    assert_eq!(
        decoder.next_data().expect("done event should parse"),
        Some(String::from("[DONE]"))
    );
}

#[test]
fn decodes_sse_with_cr_only_line_endings() {
    let mut decoder = SseDecoder::default();
    decoder
        .push(b"data: first\rdata: second\r\r")
        .expect("CR-only event should decode");

    assert_eq!(
        decoder.next_data().expect("event should parse"),
        Some(String::from("first\nsecond"))
    );
}

#[test]
fn rejects_an_sse_event_over_the_transport_limit() {
    let mut decoder = SseDecoder::default();

    let error = decoder
        .push(&vec![b'x'; MAX_SSE_EVENT_BYTES + 1])
        .expect_err("oversized event should fail");

    assert!(matches!(error, ModelError::EventTooLarge));
}

#[test]
fn parses_usage_only_chunks_and_normalizes_cache_metrics() {
    assert_eq!(
        parse_delta(
            r#"{"choices":[],"usage":{"prompt_tokens":20,"completion_tokens":5,"prompt_tokens_details":{"cached_tokens":8}}}"#,
        )
        .expect("usage chunk should parse"),
        vec![ModelEvent::Usage(ModelUsage {
            prompt_tokens: 20,
            completion_tokens: 5,
            cache_read_tokens: 8,
            cache_write_tokens: 0,
            cache_eligible_input_tokens: 20,
            cache_observed: true,
        })]
    );
    assert_eq!(
        parse_delta(
            r#"{"choices":[],"usage":{"input_tokens":3,"output_tokens":2,"cache_read_input_tokens":4}}"#,
        )
        .expect("invalid cache metrics should fail closed"),
        vec![ModelEvent::Usage(ModelUsage {
            prompt_tokens: 3,
            completion_tokens: 2,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cache_eligible_input_tokens: 0,
            cache_observed: false,
        })]
    );
}

#[tokio::test]
async fn anthropic_transport_rejects_truncation_and_times_out_idle_streams() {
    for (payload, expected) in [
        (
            "data: {\"type\":\"message_start\",\"message\":{}}\n\n",
            "Anthropic stream ended before message_stop",
        ),
        (
            "data: {\"type\":\"error\",\"error\":{\"message\":\"private-value\"}}\n\n",
            "Anthropic stream reported an error",
        ),
    ] {
        let source = Box::pin(futures_util::stream::iter(vec![Ok(Bytes::from_static(
            payload.as_bytes(),
        ))]));
        let events = model_event_stream(
            source,
            Duration::from_millis(50),
            ModelProtocol::AnthropicMessages,
            None,
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].as_ref().unwrap_err().to_string(), expected);
    }
    let source = Box::pin(futures_util::stream::pending());
    let mut events = model_event_stream(
        source,
        Duration::from_millis(20),
        ModelProtocol::AnthropicMessages,
        None,
    );
    assert!(matches!(
        events.next().await,
        Some(Err(ModelError::StreamIdleTimeout))
    ));
    assert!(events.next().await.is_none());
}

#[test]
fn openai_requests_exclude_native_history_fields() {
    let mut message = StoredMessage::text("assistant", "portable answer");
    message.provider_content.insert(String::from("anthropic-messages"), vec![serde_json::json!({"type": "thinking", "thinking": "native-only", "signature": "private-signature"})]);
    message.tool_error = Some(true);
    let runtime = ModelRuntime {
        config: ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1/v1"),
            default_model: String::from("fixture"),
        },
        options: ModelRequestOptions::default(),
    };
    let (url, body) = model_request(&runtime, "fixture", &[message], &[]).unwrap();
    assert_eq!(url, "http://127.0.0.1/v1/chat/completions");
    assert_eq!(
        body,
        serde_json::json!({"model": "fixture", "messages": [{"role": "assistant", "content": "portable answer"}],
        "stream": true, "stream_options": {"include_usage": true}, "tools": [], "tool_choice": "auto"})
    );
}

#[test]
fn model_requests_use_native_image_content_and_exclude_snapshot_metadata() {
    use qwenpaw_storage::{StoredUserInput, StoredUserPart};
    let mut message = StoredMessage::text("user", "inspect");
    message.user_input = Some(StoredUserInput {
        item_id: String::from("private-item"),
        parts: vec![
            StoredUserPart::Image {
                path: String::from("private-path.png"),
                mime_type: String::from("image/png"),
                size: 3,
                data: Some(String::from("YWJj")),
            },
            StoredUserPart::Text {
                text: String::from("inspect"),
            },
        ],
    });
    for (protocol, expected) in [
        (
            ModelProtocol::OpenAIChat,
            serde_json::json!([{"role":"user", "content":[{"type":"image_url","image_url":{"url":"data:image/png;base64,YWJj"}},{"type":"text","text":"inspect"}]}]),
        ),
        (
            ModelProtocol::AnthropicMessages,
            serde_json::json!([{"role":"user", "content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"YWJj"}},{"type":"text","text":"inspect"}]}]),
        ),
        (
            ModelProtocol::GeminiGenerateContent,
            serde_json::json!([{"role":"user", "parts":[{"inlineData":{"mimeType":"image/png","data":"YWJj"}},{"text":"inspect"}]}]),
        ),
    ] {
        let runtime = ModelRuntime {
            config: ModelConfig {
                api_key: None,
                base_url: String::from("http://127.0.0.1"),
                default_model: String::from("fixture"),
            },
            options: ModelRequestOptions {
                protocol,
                ..ModelRequestOptions::default()
            },
        };
        let (_, body) = model_request(&runtime, "fixture", &[message.clone()], &[]).unwrap();
        let key = if protocol == ModelProtocol::GeminiGenerateContent {
            "contents"
        } else {
            "messages"
        };
        assert_eq!(body[key], expected);
        for private in [
            "user_input",
            "private-item",
            "private-path",
            "mime_type",
            "size",
        ] {
            assert!(!body.to_string().contains(private), "{body}");
        }
    }
}

#[tokio::test]
async fn request_headers_override_defaults_and_errors_redact_private_values() {
    let base_url = start_server(Router::new().route(
        "/chat/completions",
        post(|headers: axum::http::HeaderMap| async move {
            assert_eq!(headers["authorization"], "Bearer private-header");
            (
                StatusCode::UNAUTHORIZED,
                "private-header private-key private-custom",
            )
        }),
    ))
    .await;
    let client = test_client(base_url, Duration::from_secs(1));
    {
        let mut runtime = client.write_runtime();
        runtime.config.api_key = Some(String::from("private-key"));
        runtime.options.custom_headers = std::collections::BTreeMap::from([
            (
                String::from("authorization"),
                String::from("Bearer private-header"),
            ),
            (String::from("x-secret"), String::from("private-custom")),
        ]);
    }
    let result = client.chat_stream("qwen-test", &test_messages(), &[]).await;
    assert!(
        matches!(result, Err(ModelError::HttpStatus { status: 401, message })
        if message == "[REDACTED] [REDACTED] [REDACTED]")
    );
}

#[tokio::test]
async fn returns_a_bounded_rate_limit_error() {
    let base_url = start_server(Router::new().route(
        "/chat/completions",
        post(|| async { (StatusCode::TOO_MANY_REQUESTS, "rate limited") }),
    ))
    .await;
    let client = test_client(base_url, Duration::from_secs(1));

    let result = client.chat_stream("qwen-test", &test_messages(), &[]).await;

    assert!(matches!(
        result,
        Err(ModelError::HttpStatus { status: 429, message }) if message == "rate limited"
    ));
}

#[tokio::test]
async fn rejects_an_oversized_http_error_body() {
    let base_url = start_server(Router::new().route(
        "/chat/completions",
        post(|| async {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(vec![b'x'; MAX_ERROR_BODY_BYTES + 1]))
                .expect("response should build")
        }),
    ))
    .await;
    let client = test_client(base_url, Duration::from_secs(1));

    let result = client.chat_stream("qwen-test", &test_messages(), &[]).await;

    assert!(matches!(result, Err(ModelError::ErrorBodyTooLarge)));
}

#[tokio::test]
async fn rejects_a_success_response_that_is_not_an_event_stream() {
    let base_url = start_server(Router::new().route(
        "/chat/completions",
        post(|| async { (StatusCode::OK, "not an event stream") }),
    ))
    .await;
    let client = test_client(base_url, Duration::from_secs(1));

    let result = client.chat_stream("qwen-test", &test_messages(), &[]).await;

    assert!(matches!(result, Err(ModelError::UnexpectedContentType)));
}

#[tokio::test]
async fn times_out_waiting_for_response_headers() {
    let base_url = start_server(Router::new().route(
        "/chat/completions",
        post(|| async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Response::builder()
                .header(CONTENT_TYPE, "text/event-stream")
                .body(Body::from("data: [DONE]\n\n"))
                .expect("response should build")
        }),
    ))
    .await;
    let client = test_client(base_url, Duration::from_millis(50));

    let result = client.chat_stream("qwen-test", &test_messages(), &[]).await;

    assert!(matches!(result, Err(ModelError::HeaderTimeout)));
}

#[tokio::test]
async fn times_out_when_an_sse_stream_is_idle() {
    let base_url = start_server(Router::new().route(
        "/chat/completions",
        post(|| async {
            let pending = futures_util::stream::pending::<Result<Bytes, Infallible>>();
            Response::builder()
                .header(CONTENT_TYPE, "text/event-stream")
                .body(Body::from_stream(pending))
                .expect("response should build")
        }),
    ))
    .await;
    let client = test_client(base_url, Duration::from_millis(50));
    let mut stream = client
        .chat_stream("qwen-test", &test_messages(), &[])
        .await
        .expect("response headers should arrive");

    let event = stream.next().await.expect("timeout should emit an error");

    assert!(matches!(event, Err(ModelError::StreamIdleTimeout)));
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn rejects_a_stream_that_ends_without_done() {
    let base_url = start_server(Router::new().route(
        "/chat/completions",
        post(|| async {
            Response::builder()
                .header(CONTENT_TYPE, "text/event-stream")
                .body(Body::from(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n",
                ))
                .expect("response should build")
        }),
    ))
    .await;
    let client = test_client(base_url, Duration::from_secs(1));
    let mut stream = client
        .chat_stream("qwen-test", &test_messages(), &[])
        .await
        .expect("response should stream");

    assert_eq!(
        stream
            .next()
            .await
            .expect("text should emit")
            .expect("text should parse"),
        ModelEvent::TextDelta(String::from("partial"))
    );
    assert!(matches!(
        stream
            .next()
            .await
            .expect("disconnect should emit an error"),
        Err(ModelError::UnexpectedEnd)
    ));
    assert!(stream.next().await.is_none());
}

fn test_client(base_url: String, timeout: Duration) -> ModelClient {
    ModelClient::with_limits(
        ModelConfig {
            api_key: None,
            base_url,
            default_model: String::from("qwen-test"),
        },
        ModelTransportLimits {
            header_timeout: timeout,
            stream_idle_timeout: timeout,
        },
    )
    .expect("test client should build")
}

fn test_messages() -> Vec<StoredMessage> {
    vec![StoredMessage::text("user", "hello")]
}

async fn start_server(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let address = listener
        .local_addr()
        .expect("listener should have an address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("test server should run");
    });
    format!("http://{address}")
}
