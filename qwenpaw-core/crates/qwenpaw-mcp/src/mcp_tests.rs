use std::sync::Arc;

use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

struct UnexpectedCredentialStore;

impl McpOAuthCredentialStore for UnexpectedCredentialStore {
    fn load(&self, _account: &str) -> Result<Option<McpOAuthCredentials>, String> {
        panic!("plain HTTP MCP must not read the OAuth credential store");
    }

    fn save(&self, _account: &str, _credentials: &McpOAuthCredentials) -> Result<(), String> {
        panic!("plain HTTP MCP must not write the OAuth credential store");
    }

    fn delete(&self, _account: &str) -> Result<(), String> {
        panic!("plain HTTP MCP must not delete from the OAuth credential store");
    }
}

#[test]
fn normalizes_remote_transport_aliases_and_inference() {
    for (raw, expected) in [
        (
            json!({"type": "http", "url": "https://example.test/mcp"}),
            "streamable_http",
        ),
        (
            json!({"transport": "streamable-http", "url": "https://example.test/mcp"}),
            "streamable_http",
        ),
        (
            json!({"transport": "sse", "url": "https://example.test/sse"}),
            "sse",
        ),
        (
            json!({"url": "https://example.test/mcp"}),
            "streamable_http",
        ),
    ] {
        let mut config: McpClientConfig =
            serde_json::from_value(raw).expect("test config should deserialize");
        normalize_transport(&mut config);
        assert_eq!(config.transport, expected);
    }
}

#[test]
fn parses_multiline_sse_and_rejects_cross_origin_endpoints() {
    assert_eq!(
        sse_event_end(b"event: message\r\ndata: one\r\n\r\nrest"),
        Some((25, 4))
    );
    let event = parse_sse_event(b"event: message\ndata: first\ndata: second")
        .expect("SSE should parse")
        .expect("SSE should contain data");
    assert_eq!(event.kind, "message");
    assert_eq!(event.data, "first\nsecond");

    let base = reqwest::Url::parse("https://example.test/sse").expect("base URL");
    assert_eq!(
        validate_legacy_endpoint(&base, "/messages")
            .expect("same-origin endpoint")
            .as_str(),
        "https://example.test/messages"
    );
    assert!(validate_legacy_endpoint(&base, "https://attacker.test/messages").is_err());
}

#[test]
fn marks_remote_headers_sensitive() {
    let mut config: McpClientConfig = serde_json::from_value(json!({
        "transport": "streamable_http",
        "url": "https://example.test/mcp",
        "headers": {"X-Api-Key": "secret"}
    }))
    .expect("test config should deserialize");
    normalize_transport(&mut config);
    let (headers, token) = resolve_http_headers(&config).expect("headers should resolve");
    assert!(token.is_none());
    assert!(
        headers
            .get(&HeaderName::from_static("x-api-key"))
            .expect("header should exist")
            .is_sensitive()
    );
}

#[tokio::test]
async fn plain_http_clients_do_not_access_the_oauth_credential_store() {
    let mut config: McpClientConfig = serde_json::from_value(json!({
        "transport": "streamable_http",
        "url": "http://127.0.0.1:3000/mcp"
    }))
    .expect("test config should deserialize");
    normalize_transport(&mut config);
    let manager = McpManager::new(
        BTreeMap::from([(String::from("plain"), config.clone())]),
        Arc::new(UnexpectedCredentialStore),
    );

    assert_eq!(
        manager.clients().await.expect("client inventory")[0].oauth_status,
        None
    );
    assert_eq!(
        resolve_manager_bearer(
            &manager,
            "plain",
            &config,
            Some(String::from("configured-token")),
            &reqwest::Client::new(),
        )
        .await
        .expect("plain HTTP bearer should resolve"),
        Some(String::from("configured-token"))
    );
}
