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

fn test_settings(key: &str) -> McpClientSettings {
    McpClientSettings {
        key: key.to_owned(),
        name: String::from("Test MCP"),
        description: String::new(),
        enabled: false,
        transport: String::from("http"),
        url: String::from("https://example.test/mcp"),
        headers: HashMap::from([(String::from("Authorization"), String::from("secret"))]),
        command: String::new(),
        args: Vec::new(),
        env: HashMap::from([(String::from("TOKEN"), String::from("secret"))]),
        cwd: String::new(),
        tools: None,
        oauth: Some(McpOAuthSettings::default()),
        access: McpAccessPolicy::default(),
    }
}

#[tokio::test]
async fn reconfiguration_normalizes_transport_and_redacts_secrets() {
    let manager = McpManager::empty()
        .reconfigured(vec![test_settings("remote")])
        .expect("configuration should be valid");

    let settings = manager.settings();
    assert_eq!(settings[0].transport, "streamable_http");
    assert_eq!(settings[0].headers["Authorization"], "secret");
    assert_eq!(settings[0].env["TOKEN"], "secret");
    let clients = manager.clients().await.expect("clients should list");
    assert_eq!(clients[0].headers["Authorization"], "********");
    assert_eq!(clients[0].env["TOKEN"], "********");
}

#[test]
fn access_policy_uses_most_specific_console_rule() {
    let policy = McpAccessPolicy {
        default_effect: McpAccessEffect::Deny,
        client_overrides: vec![McpAccessRule {
            source_type: String::from("channel"),
            source_value: String::from("console"),
            subject_type: String::from("all"),
            subject_value: String::new(),
            effect: McpAccessEffect::Ask,
        }],
        tool_defaults: vec![McpToolDefaultPolicy {
            tool_name: String::from("read"),
            effect: McpAccessEffect::Allow,
        }],
        tool_overrides: vec![McpToolAccessOverride {
            tool_name: String::from("read"),
            rule: McpAccessRule {
                source_type: String::from("channel"),
                source_value: String::from("console"),
                subject_type: String::from("user"),
                subject_value: String::from("thread-1"),
                effect: McpAccessEffect::Deny,
            },
        }],
        unmanaged_rules_count: 0,
    };

    assert_eq!(
        resolve_access_effect(&policy, "read", "console", "thread-1"),
        McpAccessEffect::Deny
    );
    assert_eq!(
        resolve_access_effect(&policy, "read", "console", "thread-2"),
        McpAccessEffect::Allow
    );
    assert_eq!(
        resolve_access_effect(&policy, "write", "console", "thread-2"),
        McpAccessEffect::Ask
    );
    assert_eq!(
        resolve_access_effect(&policy, "write", "remote", "thread-2"),
        McpAccessEffect::Deny
    );
}

#[test]
fn reconfiguration_rejects_invalid_policy_and_duplicate_keys() {
    let manager = McpManager::empty();
    let mut invalid = test_settings("remote");
    invalid.access.client_overrides.push(McpAccessRule {
        source_type: String::from("channel"),
        source_value: String::from("console"),
        subject_type: String::from("user"),
        subject_value: String::new(),
        effect: McpAccessEffect::Allow,
    });
    assert!(manager.reconfigured(vec![invalid]).is_err());
    assert!(
        manager
            .reconfigured(vec![test_settings("same"), test_settings("same")])
            .is_err()
    );
}
