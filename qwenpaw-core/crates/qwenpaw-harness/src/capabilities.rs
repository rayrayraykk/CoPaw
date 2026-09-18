//! Resolved in-memory capabilities, separate from credential/file discovery.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CapabilityError {
    #[error("Capability path is not valid Unicode")]
    InvalidPath,
    #[error("Codex MCP servers require conflicting values for environment variable {0}.")]
    EnvironmentConflict(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    #[default]
    Stdio,
    StreamableHttp,
    Sse,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPolicy {
    Allow,
    #[default]
    Ask,
    Deny,
}

#[derive(Clone, Default)]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub directory: PathBuf,
    pub revision: String,
}

/// Resolved values may be secrets. No whole-object Debug or Serialize is provided.
#[derive(Clone, Default)]
pub struct McpServerDefinition {
    pub name: String,
    pub display_name: String,
    pub transport: McpTransport,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub url: String,
    pub env: IndexMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub tools: Option<Vec<String>>,
    pub tool_policies: BTreeMap<String, ToolPolicy>,
    pub default_policy: ToolPolicy,
    pub credential_revision: String,
    pub runtime_revision: String,
}

impl McpServerDefinition {
    /// Refreshes the original resolved-value digest after env/header resolution.
    /// This is not a credential-store lookup or a background refresh mechanism.
    pub fn refresh_runtime_revision(&mut self) {
        self.runtime_revision =
            sha256(json_ascii(&json!({"env":self.env,"headers":self.headers}), false).as_bytes());
    }

    fn fingerprint_payload(&self) -> Result<Value, CapabilityError> {
        let mut env_keys: Vec<_> = self.env.keys().collect();
        env_keys.sort_unstable();
        Ok(json!({
            "name":self.name,"transport":self.transport,"command":self.command,
            "args":self.args,"cwd":self.cwd.as_deref().map(path_text).transpose()?.unwrap_or(""),
            "url":self.url,"env_keys":env_keys,
            "header_keys":self.headers.keys().collect::<Vec<_>>(),"tools":self.tools,
            "tool_policies":self.tool_policies,"default_policy":self.default_policy,
            "credential_revision":self.credential_revision,"runtime_revision":self.runtime_revision
        }))
    }
}

/// Effective capabilities supplied by the workspace resolver, not raw UI input.
#[derive(Clone, Default)]
pub struct RuntimeCapabilities {
    pub skills: Vec<SkillDefinition>,
    pub mcp_servers: Vec<McpServerDefinition>,
}

impl RuntimeCapabilities {
    /// Computes the original client-isolation key from metadata and revisions.
    /// Resolved env/header values are excluded, but inline URL/arg data is not redacted.
    ///
    /// # Errors
    /// Rejects non-Unicode paths instead of silently changing client identity.
    pub fn fingerprint(&self) -> Result<String, CapabilityError> {
        let skills = self.skills.iter().map(|skill| {
            Ok(json!({"name":skill.name,"directory":path_text(&skill.directory)?,"revision":skill.revision}))
        }).collect::<Result<Vec<_>, CapabilityError>>()?;
        let servers = self
            .mcp_servers
            .iter()
            .map(McpServerDefinition::fingerprint_payload)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sha256(
            json_ascii(&json!({"skills":skills,"mcp_servers":servers}), false).as_bytes(),
        ))
    }
}

pub(crate) fn path_text(path: &Path) -> Result<&str, CapabilityError> {
    path.to_str().ok_or(CapabilityError::InvalidPath)
}

pub(crate) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Python-compatible ASCII JSON for typed capability data, with sorted object keys.
pub(crate) fn json_ascii(value: &Value, spaces: bool) -> String {
    let separator = if spaces { ", " } else { "," };
    match value {
        Value::String(value) => {
            let encoded = serde_json::to_string(value).expect("strings serialize to JSON");
            let mut ascii = String::new();
            for character in encoded.chars() {
                if character < '\u{7f}' {
                    ascii.push(character);
                } else {
                    for unit in character.encode_utf16(&mut [0; 2]) {
                        write!(ascii, "\\u{unit:04x}").expect("writing to String cannot fail");
                    }
                }
            }
            ascii
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(|value| json_ascii(value, spaces))
                .collect::<Vec<_>>()
                .join(separator)
        ),
        Value::Object(values) => {
            let mut entries: Vec<_> = values.iter().collect();
            entries.sort_unstable_by_key(|(key, _)| *key);
            let colon = if spaces { ": " } else { ":" };
            format!(
                "{{{}}}",
                entries
                    .into_iter()
                    .map(|(key, value)| format!(
                        "{}{colon}{}",
                        json_ascii(&json!(key), spaces),
                        json_ascii(value, spaces)
                    ))
                    .collect::<Vec<_>>()
                    .join(separator)
            )
        }
        _ => value.to_string(),
    }
}

#[cfg(test)]
pub(crate) mod tests;
