use std::collections::BTreeMap;
use std::fs;

use pretty_assertions::assert_eq;
use qwenpaw_mcp::McpManager;
use serde_json::json;

#[tokio::test]
async fn stdio_uses_snapshot_environment_and_client_overrides_without_changing_the_host() {
    let keys = [
        "QWENPAW_MCP_INHERITED_FIXTURE",
        "QWENPAW_MCP_EXPANDED_FIXTURE",
        "QWENPAW_MCP_OVERRIDE_FIXTURE",
    ];
    let host_before = keys.map(std::env::var_os);
    let settings = serde_json::from_value(json!([{
        "key": "environment", "enabled": true, "transport": "stdio",
        "command": env!("CARGO_BIN_EXE_qwenpaw-mcp-test-server"),
        "env": {"QWENPAW_MCP_EXPANDED_FIXTURE": "${QWENPAW_MCP_INHERITED_FIXTURE}",
                "QWENPAW_MCP_OVERRIDE_FIXTURE": "client-value"}
    }]))
    .unwrap();
    let original = McpManager::empty().reconfigured(settings).unwrap();
    let values = BTreeMap::from([
        (keys[0].to_owned(), String::from("original-value")),
        (keys[2].to_owned(), String::from("application-value")),
    ]);
    let first = original.with_environment(values.clone());
    let mut next = values;
    next.insert(keys[0].to_owned(), String::from("restored-value"));
    let restored = first.with_environment(next);
    for (manager, value) in [
        (&first, "original-value"),
        (&restored, "restored-value"),
        (&first, "original-value"),
    ] {
        manager.validate_environment_bindings().unwrap();
        assert_eq!(manager.definitions().await.len(), 1);
        let output = manager
            .call_tool(
                "mcp__environment__echo",
                r#"{"text":"__qwenpaw_env_fixture__"}"#,
            )
            .await
            .unwrap();
        assert!(!output.is_error);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&output.content).unwrap(),
            json!({"environment": {
                "inherited": value, "expanded": value, "override": "client-value"
            }})
        );
    }
    first.cancel_tool("mcp__environment__echo").await;
    restored.cancel_tool("mcp__environment__echo").await;
    assert_eq!(keys.map(std::env::var_os), host_before);
}

#[tokio::test]
async fn discovers_and_calls_a_namespaced_stdio_tool() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let config_path = directory.path().join("mcp.json");
    fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "clients": {
                "echo-server": {
                    "name": "echo",
                    "enabled": true,
                    "transport": "stdio",
                    "command": env!("CARGO_BIN_EXE_qwenpaw-mcp-test-server"),
                    "args": [],
                    "env": {},
                    "tools": ["echo"]
                }
            }
        }))
        .expect("serialize config"),
    )
    .expect("write config");
    let manager = McpManager::from_path(&config_path).expect("load MCP config");

    let definitions = manager.definitions().await;

    assert_eq!(
        definitions,
        vec![json!({
            "type": "function",
            "function": {
                "name": "mcp__echo-server__echo",
                "description": "Echo a text value",
                "parameters": {
                    "type": "object",
                    "properties": {"text": {"type": "string"}},
                    "required": ["text"]
                }
            }
        })]
    );
    assert!(manager.contains_tool("mcp__echo-server__echo").await);
    assert_eq!(
        manager
            .call_tool("mcp__echo-server__echo", r#"{"text":"hello"}"#)
            .await
            .expect("call MCP tool"),
        qwenpaw_mcp::McpToolOutput {
            content: String::from(r#"{"echo":"hello"}"#),
            is_error: false,
        }
    );
}

#[tokio::test]
async fn cancellation_terminates_an_in_flight_stdio_call() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let config_path = write_config(directory.path());
    let manager = McpManager::from_path(&config_path).expect("load MCP config");
    assert_eq!(manager.definitions().await.len(), 1);
    let calling_manager = manager.clone();
    let call = tokio::spawn(async move {
        calling_manager
            .call_tool("mcp__echo-server__echo", r#"{"text":"slow"}"#)
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let cancelled_at = std::time::Instant::now();

    manager.cancel_tool("mcp__echo-server__echo").await;

    let result = tokio::time::timeout(std::time::Duration::from_secs(5), call)
        .await
        .expect("cancelled MCP call should honor the transport shutdown grace period")
        .expect("MCP task should join");
    assert!(result.is_err());
    assert!(cancelled_at.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn accepts_the_legacy_agent_json_wrapper_and_sse_clients() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let config_path = directory.path().join("agent.json");
    fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "mcp": {
                "clients": {
                    "remote": {
                        "name": "remote",
                        "enabled": true,
                        "transport": "sse",
                        "url": "https://example.invalid/sse"
                    }
                }
            }
        }))
        .expect("serialize config"),
    )
    .expect("write config");

    assert!(
        !McpManager::from_path(&config_path)
            .expect("load wrapped MCP config")
            .is_empty()
    );
}

fn write_config(directory: &std::path::Path) -> std::path::PathBuf {
    let config_path = directory.join("mcp.json");
    fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "clients": {
                "echo-server": {
                    "enabled": true,
                    "transport": "stdio",
                    "command": env!("CARGO_BIN_EXE_qwenpaw-mcp-test-server"),
                    "tools": ["echo"]
                }
            }
        }))
        .expect("serialize config"),
    )
    .expect("write config");
    config_path
}
