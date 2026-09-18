//! Original Codex runtime configuration projection from resolved capabilities.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::capabilities::{
    CapabilityError, McpServerDefinition, McpTransport, RuntimeCapabilities, ToolPolicy,
    json_ascii, path_text, sha256,
};

/// In-memory process configuration. Contains secrets; intentionally not Debug/Serialize.
#[derive(Clone, Default)]
pub struct RuntimeProjection {
    pub config_overrides: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub skill_roots: Vec<String>,
}

/// Projects original MCP settings and skill roots, without starting any process.
///
/// # Errors
/// Rejects conflicting shared stdio env values and non-Unicode paths.
pub fn project_runtime(
    capabilities: &RuntimeCapabilities,
) -> Result<RuntimeProjection, CapabilityError> {
    let mut result = RuntimeProjection::default();
    let mut forwarded = BTreeMap::new();
    for server in &capabilities.mcp_servers {
        let prefix = format!("mcp_servers.{}", config_key(&server.name));
        if server.transport == McpTransport::Stdio {
            result
                .config_overrides
                .push(setting(&prefix, "command", &json!(server.command)));
            result
                .config_overrides
                .push(setting(&prefix, "args", &json!(server.args)));
            if let Some(cwd) = &server.cwd {
                result
                    .config_overrides
                    .push(setting(&prefix, "cwd", &json!(path_text(cwd)?)));
            }
            for (name, value) in &server.env {
                if forwarded
                    .get(name)
                    .is_some_and(|previous| previous != value)
                {
                    return Err(CapabilityError::EnvironmentConflict(name.clone()));
                }
                forwarded.insert(name.clone(), value.clone());
                result.environment.insert(name.clone(), value.clone());
            }
            if !server.env.is_empty() {
                let mut names: Vec<_> = server.env.keys().collect();
                names.sort_unstable();
                result
                    .config_overrides
                    .push(setting(&prefix, "env_vars", &json!(names)));
            }
        } else {
            result
                .config_overrides
                .push(setting(&prefix, "url", &json!(server.url)));
            let mut headers = BTreeMap::new();
            for (header, value) in &server.headers {
                let name = header_env_name(&server.name, header);
                result.environment.insert(name.clone(), value.clone());
                headers.insert(header.clone(), name);
            }
            if !headers.is_empty() {
                result
                    .config_overrides
                    .push(setting(&prefix, "env_http_headers", &json!(headers)));
            }
        }
        tool_policy(&mut result.config_overrides, &prefix, server);
    }
    result.skill_roots = capabilities
        .skills
        .iter()
        .map(|skill| path_text(&skill.directory).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}

fn tool_policy(overrides: &mut Vec<String>, prefix: &str, server: &McpServerDefinition) {
    if let Some(tools) = &server.tools {
        let enabled: Vec<_> = tools
            .iter()
            .filter(|name| {
                server
                    .tool_policies
                    .get(*name)
                    .unwrap_or(&server.default_policy)
                    != &ToolPolicy::Deny
            })
            .collect();
        overrides.push(setting(prefix, "enabled_tools", &json!(enabled)));
    } else if server.default_policy == ToolPolicy::Deny {
        overrides.push(setting(prefix, "enabled_tools", &json!([])));
    }
    overrides.push(setting(
        prefix,
        "default_tools_approval_mode",
        &json!(approval(server.default_policy)),
    ));
    for (name, effect) in &server.tool_policies {
        if *effect != ToolPolicy::Deny && valid_key(name) {
            overrides.push(setting(
                &format!("{prefix}.tools.{name}"),
                "approval_mode",
                &json!(approval(*effect)),
            ));
        }
    }
}

fn approval(policy: ToolPolicy) -> &'static str {
    if policy == ToolPolicy::Allow {
        "approve"
    } else {
        "prompt"
    }
}

fn setting(prefix: &str, name: &str, value: &Value) -> String {
    format!("{prefix}.{name}={}", toml_value(value))
}

fn toml_value(value: &Value) -> String {
    if let Value::Object(values) = value {
        let mut entries: Vec<_> = values.iter().collect();
        entries.sort_unstable_by_key(|(key, _)| *key);
        let values = entries
            .into_iter()
            .map(|(key, value)| {
                format!("{} = {}", json_ascii(&json!(key), false), toml_value(value))
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {values} }}")
    } else {
        json_ascii(value, true)
    }
}

fn valid_key(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || value == '_' || value == '-')
}

fn normalized(name: &str) -> String {
    name.chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() || value == '_' {
                value
            } else {
                '_'
            }
        })
        .collect()
}

fn config_key(name: &str) -> String {
    if valid_key(name) {
        return name.to_owned();
    }
    let name_normalized = normalized(name);
    let name_normalized = name_normalized.trim_matches('_');
    let name_normalized = if name_normalized.is_empty() {
        "server"
    } else {
        name_normalized
    };
    format!(
        "{}_{}",
        &name_normalized[..name_normalized.len().min(32)],
        &sha256(name.as_bytes())[..10]
    )
}

fn header_env_name(server: &str, header: &str) -> String {
    let normalized = normalized(&format!("{server}_{header}")).to_ascii_uppercase();
    let digest = sha256(format!("{server}:{header}").as_bytes());
    format!(
        "QWENPAW_MCP_{}_{}",
        &normalized[..normalized.len().min(32)],
        digest[..12].to_ascii_uppercase()
    )
}

#[cfg(test)]
mod tests;
