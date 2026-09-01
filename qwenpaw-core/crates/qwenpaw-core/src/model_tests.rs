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
