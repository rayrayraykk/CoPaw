use serde_json::Value;
use serde_json::json;

use super::*;

fn payload() -> Value {
    json!({"version": 1, "api_key": "model-secret",
        "environment": {"LINES": "line1\nline2", "EMPTY": ""},
        "agent_settings": {
            "running-config.embedding-api-key": "embedding-secret",
            "agent.writer.running-config.reranker-api-key": "reranker-secret",
            "agent.writer.mail-auth-code": "mail-secret"
        },
        "model_providers": {"custom-provider": "provider-secret"},
        "mcp_clients": {"remote": json!({"headers": {"Authorization": "Bearer token"},
            "env": {"MCP-NAME": "multi\nline"}, "oauth_access_token": "", "oauth_refresh_token": ""}).to_string()},
        "oauth": {"version": 1, "clients": {"remote": null}}
    })
}

#[test]
fn validation_preserves_values_and_accepts_all_supported_credential_namespaces() {
    let original = payload();
    let mut snapshot: SecretSnapshot = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(snapshot.validate(), Ok(()));
    assert_eq!(serde_json::to_value(&snapshot).unwrap(), original);
    for id in ["default", "writer"] {
        for key in desktop_agent_settings::backup_secret_keys(id) {
            snapshot.agent_settings.insert(key, String::from("secret"));
        }
    }
    assert_eq!(snapshot.validate(), Ok(()));
}

#[test]
fn unknown_fields_and_missing_required_domains_cannot_authorize_deletion() {
    for key in [
        "version",
        "api_key",
        "environment",
        "agent_settings",
        "mcp_clients",
        "model_providers",
    ] {
        let mut value = payload();
        value.as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<SecretSnapshot>(value).is_err(),
            "{key}"
        );
    }
    let mut value = payload();
    value["backup_signing_key"] = json!("must-not-be-restored");
    assert!(serde_json::from_value::<SecretSnapshot>(value).is_err());
    let mut value = payload();
    value["api_key"] = Value::Null;
    value.as_object_mut().unwrap().remove("oauth");
    let snapshot: SecretSnapshot = serde_json::from_value(value).unwrap();
    assert_eq!(snapshot.validate(), Ok(()));
    assert!(snapshot.oauth.is_none());
}

#[test]
fn rejects_invalid_versions_namespace_injection_aliases_and_credential_values() {
    for (field, value) in [
        ("version", json!(2)),
        ("api_key", json!("secret\nheader-injection")),
        ("api_key", json!("a".repeat(8193))),
        ("environment", json!({"INVALID=KEY": "secret"})),
        ("environment", json!({"VALUE": "bad\0value"})),
        ("environment", json!({"VALUE": "a".repeat(65_537)})),
        ("agent_settings", json!({"backup-signing-key": "secret"})),
        (
            "agent_settings",
            json!({"model-provider-api-key:provider": "secret"}),
        ),
        (
            "agent_settings",
            json!({"agent.default.running-config.embedding-api-key": "alias"}),
        ),
        (
            "agent_settings",
            json!({"agent../outside.mail-auth-code": "secret"}),
        ),
        ("agent_settings", json!({"agent.writer.unknown": "secret"})),
        (
            "agent_settings",
            json!({"running-config.embedding-api-key": "a".repeat(16_385)}),
        ),
        (
            "model_providers",
            json!({"openai-compatible": "duplicate-default"}),
        ),
        ("model_providers", json!({"../outside": "secret"})),
        ("mcp_clients", json!({"\ninvalid": "{}"})),
        ("mcp_clients", json!({"remote": "{}"})),
        ("oauth", json!({"version": 2, "clients": {}})),
        (
            "oauth",
            json!({"version": 1, "clients": {"\ninvalid": null}}),
        ),
    ] {
        let mut payload = payload();
        payload[field] = value;
        let snapshot: SecretSnapshot = serde_json::from_value(payload).unwrap();
        assert!(snapshot.validate().is_err(), "{field}");
    }
}

#[test]
fn nested_mcp_secrets_reject_extra_fields_header_injection_and_invalid_process_environment() {
    for (field, value) in [
        ("unexpected", json!("secret")),
        ("headers", json!({"Bad\nName": "secret"})),
        (
            "headers",
            json!({"Authorization": "Bearer token\nInjected: secret"}),
        ),
        ("headers", json!({"X-Large": "a".repeat(16_384)})),
        ("env", json!({"NAME=VALUE": "secret"})),
        ("env", json!({"NAME": "secret\0bad"})),
        ("oauth_refresh_token", json!("secret\ninvalid")),
    ] {
        let mut value_original = payload();
        let mut secrets: Value =
            serde_json::from_str(value_original["mcp_clients"]["remote"].as_str().unwrap())
                .unwrap();
        secrets[field] = value;
        value_original["mcp_clients"]["remote"] = json!(secrets.to_string());
        let snapshot: SecretSnapshot = serde_json::from_value(value_original).unwrap();
        assert!(snapshot.validate().is_err(), "{field}");
    }
}

#[test]
fn domain_counts_are_bounded_before_runtime_hydration() {
    for field in [
        "environment",
        "agent_settings",
        "model_providers",
        "mcp_clients",
    ] {
        let count = match field {
            "environment" => 257,
            "agent_settings" => 1025,
            "model_providers" => 129,
            _ => 33,
        };
        let mut value = payload();
        value[field] = (0..count)
            .map(|index| (format!("key{index}"), json!("secret")))
            .collect::<serde_json::Map<_, _>>()
            .into();
        let snapshot: SecretSnapshot = serde_json::from_value(value).unwrap();
        assert!(snapshot.validate().is_err(), "{field}");
    }
}

#[test]
fn independent_secret_scope_restores_all_domains_and_retains_protected_mcp_credentials() {
    let snapshot: SecretSnapshot = serde_json::from_value(payload()).unwrap();
    let known = BTreeSet::from([
        CredentialKey::ApiKey,
        CredentialKey::Environment(String::from("REMOVED")),
        CredentialKey::AgentSetting(String::from("agent.default.mail-auth-code")),
        CredentialKey::AgentSetting(String::from("model-provider-api-key:removed")),
        CredentialKey::McpClient(String::from("local-only")),
    ]);
    for preserve in [false, true] {
        let plan = plan_restore(Some(&snapshot), true, preserve, &known).unwrap();
        let mut expected = BTreeMap::from([
            (CredentialKey::ApiKey, Some(String::from("model-secret"))),
            (CredentialKey::Environment(String::from("REMOVED")), None),
            (
                CredentialKey::Environment(String::from("EMPTY")),
                Some(String::new()),
            ),
            (
                CredentialKey::Environment(String::from("LINES")),
                Some(String::from("line1\nline2")),
            ),
            (
                CredentialKey::AgentSetting(String::from("agent.default.mail-auth-code")),
                None,
            ),
            (
                CredentialKey::AgentSetting(String::from("model-provider-api-key:removed")),
                None,
            ),
            (
                CredentialKey::AgentSetting(String::from("model-provider-api-key:custom-provider")),
                Some(String::from("provider-secret")),
            ),
            (
                CredentialKey::AgentSetting(String::from("running-config.embedding-api-key")),
                Some(String::from("embedding-secret")),
            ),
            (
                CredentialKey::AgentSetting(String::from(
                    "agent.writer.running-config.reranker-api-key",
                )),
                Some(String::from("reranker-secret")),
            ),
            (
                CredentialKey::AgentSetting(String::from("agent.writer.mail-auth-code")),
                Some(String::from("mail-secret")),
            ),
        ]);
        if !preserve {
            expected.insert(CredentialKey::McpClient(String::from("local-only")), None);
            expected.insert(
                CredentialKey::McpClient(String::from("remote")),
                Some(snapshot.mcp_clients["remote"].clone()),
            );
        }
        assert_eq!(plan.credentials, expected);
        assert_eq!(
            plan.oauth,
            if preserve {
                None
            } else {
                snapshot.oauth.clone()
            }
        );
    }
}

#[test]
fn absent_or_disabled_secret_scope_never_authorizes_a_local_key_deletion() {
    let mut snapshot: SecretSnapshot = serde_json::from_value(payload()).unwrap();
    snapshot.version = 2;
    let known = BTreeSet::from([CredentialKey::ApiKey]);
    for (payload, selected) in [(None, true), (None, false), (Some(&snapshot), false)] {
        let plan = plan_restore(payload, selected, false, &known).unwrap();
        assert_eq!(plan.credentials, BTreeMap::new());
        assert_eq!(plan.oauth, None);
    }
    assert!(plan_restore(Some(&snapshot), true, true, &known).is_err());
}
