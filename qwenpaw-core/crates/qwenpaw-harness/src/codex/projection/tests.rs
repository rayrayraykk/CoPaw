use super::*;
use crate::capabilities::tests::{cases, fixture};
use indexmap::IndexMap;

fn output(capabilities: &RuntimeCapabilities) -> Value {
    match project_runtime(capabilities) {
        Err(error) => json!({"error":error.to_string()}),
        Ok(projection) => json!({"fingerprint":capabilities.fingerprint().unwrap(),
            "runtime_revisions":capabilities.mcp_servers.iter().map(|server| &server.runtime_revision).collect::<Vec<_>>(),
            "projection":{"config_overrides":projection.config_overrides,"environment":projection.environment,"skill_roots":projection.skill_roots}}),
    }
}

#[test]
fn plain_stdio_policy_has_exact_overrides_and_no_resolved_secret_values() {
    let capabilities = RuntimeCapabilities {
        mcp_servers: vec![McpServerDefinition {
            name: "local".to_owned(),
            command: "tool".to_owned(),
            args: vec!["a".to_owned(), "中".to_owned()],
            env: IndexMap::from([("TOKEN".to_owned(), "fixture-secret".to_owned())]),
            tools: Some(vec![
                "read".to_owned(),
                "blocked".to_owned(),
                "read".to_owned(),
                "tool.dot".to_owned(),
            ]),
            tool_policies: BTreeMap::from([
                ("read".to_owned(), ToolPolicy::Allow),
                ("blocked".to_owned(), ToolPolicy::Deny),
                ("tool.dot".to_owned(), ToolPolicy::Allow),
            ]),
            ..McpServerDefinition::default()
        }],
        ..RuntimeCapabilities::default()
    };
    let result = project_runtime(&capabilities).unwrap();
    assert_eq!(
        json!({"overrides":result.config_overrides,"environment":result.environment,"roots":result.skill_roots}),
        json!({
            "overrides":["mcp_servers.local.command=\"tool\"",r#"mcp_servers.local.args=["a", "\u4e2d"]"#,
                "mcp_servers.local.env_vars=[\"TOKEN\"]",r#"mcp_servers.local.enabled_tools=["read", "read", "tool.dot"]"#,
                "mcp_servers.local.default_tools_approval_mode=\"prompt\"","mcp_servers.local.tools.read.approval_mode=\"approve\""],
            "environment":{"TOKEN":"fixture-secret"},"roots":[]
        })
    );
}

#[test]
fn conflicting_stdio_values_error_without_exposing_either_value() {
    let capabilities = cases().pop().unwrap();
    assert_eq!(
        project_runtime(&capabilities).err(),
        Some(CapabilityError::EnvironmentConflict("EMPTY".to_owned()))
    );
    assert_eq!(
        output(&capabilities),
        json!({"error":"Codex MCP servers require conflicting values for environment variable EMPTY."})
    );
}

#[test]
fn deny_default_empty_tools_and_header_collisions_preserve_original_rules() {
    let result = project_runtime(&RuntimeCapabilities {
        mcp_servers: vec![McpServerDefinition {
            name: "deny".to_owned(),
            default_policy: ToolPolicy::Deny,
            tool_policies: BTreeMap::from([("override".to_owned(), ToolPolicy::Allow)]),
            ..McpServerDefinition::default()
        }],
        ..RuntimeCapabilities::default()
    })
    .unwrap();
    assert_eq!(
        result.config_overrides,
        vec![
            "mcp_servers.deny.command=\"\"",
            "mcp_servers.deny.args=[]",
            "mcp_servers.deny.enabled_tools=[]",
            "mcp_servers.deny.default_tools_approval_mode=\"prompt\"",
            "mcp_servers.deny.tools.override.approval_mode=\"approve\""
        ]
    );
    assert_ne!(
        header_env_name("http", "x:y"),
        header_env_name("http", "x/y")
    );
    assert_ne!(config_key("a.b"), config_key("a/b"));
    assert_eq!(config_key("valid-name_"), "valid-name_");
}

#[test]
fn projection_does_not_deduplicate_skill_roots_or_persist_secret_maps() {
    let mut capabilities = cases().remove(1);
    capabilities.skills.push(capabilities.skills[0].clone());
    let result = project_runtime(&capabilities).unwrap();
    assert_eq!(
        result.skill_roots,
        capabilities
            .skills
            .iter()
            .map(|skill| skill.directory.to_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    for line in result.config_overrides {
        for secret in ["fake-secret-one", "fake-bearer", "header-fixture"] {
            assert!(!line.contains(secret));
        }
    }
}

#[test]
fn first_conflicting_variable_follows_input_order_not_sorted_names() {
    let cases = cases();
    assert_eq!(
        output(&cases[cases.len() - 2]),
        json!({"error":"Codex MCP servers require conflicting values for environment variable Z."})
    );
}

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; compare original projection and fingerprints"]
async fn projection_and_fingerprints_match_original_python() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/codex_projection_reference.py");
    for capabilities in cases() {
        for refresh in [false, true] {
            let request =
                json!({"capabilities":fixture(&capabilities),"refresh_runtime_revisions":refresh});
            let mut actual = capabilities.clone();
            if refresh {
                for server in &mut actual.mcp_servers {
                    server.refresh_runtime_revision();
                }
            }
            let process = tokio::time::timeout(
                std::time::Duration::from_secs(20),
                tokio::process::Command::new("python")
                    .arg(&script)
                    .arg(request.to_string())
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(
                process.status.success(),
                "reference error: {}",
                String::from_utf8_lossy(&process.stderr)
            );
            assert_eq!(
                output(&actual),
                serde_json::from_slice::<Value>(&process.stdout).unwrap()
            );
        }
    }
}
