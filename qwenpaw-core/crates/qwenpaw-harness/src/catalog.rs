//! Original provider declarations, not proof that an Agent backend is wired.

use serde::Serialize;
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "These independent flags mirror the original public wire contract, not mutually exclusive states"
)]
pub struct HarnessCapabilities {
    pub authentication: bool,
    pub model_selection: bool,
    pub reasoning_effort: bool,
    pub reasoning_stream: bool,
    pub tool_stream: bool,
    pub session_resume: bool,
    pub workspace_ui: bool,
    pub native_skills_ui: bool,
    pub native_tools_ui: bool,
    pub native_mcp_ui: bool,
    pub loop_modes: bool,
    pub attachments: bool,
    pub context_usage: bool,
    pub skills_commands: bool,
    pub qwenpaw_skills_projection: bool,
    pub qwenpaw_mcp_projection: bool,
    pub provider_skills_discovery: bool,
    pub provider_mcp_discovery: bool,
    pub mcp_tool_allowlist: bool,
    pub commands: Vec<HarnessCommand>,
    pub approval_presets: Vec<HarnessApprovalPreset>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HarnessCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub accepts_arguments: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HarnessApprovalPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub settings: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProviderCatalogItem {
    pub id: &'static str,
    pub name: &'static str,
    pub coming_soon: bool,
    pub capabilities: HarnessCapabilities,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProviderStatus {
    #[serde(flatten)]
    pub declaration: ProviderCatalogItem,
    pub available: bool,
    pub installed: bool,
    pub authenticated: bool,
    pub account: Option<Map<String, Value>>,
    pub runtime_path: Option<String>,
    pub runtime_source: Option<String>,
    pub error: Option<String>,
}

impl ProviderCatalogItem {
    pub(crate) fn status(self) -> ProviderStatus {
        ProviderStatus {
            available: !self.coming_soon,
            declaration: self,
            installed: false,
            authenticated: false,
            account: None,
            runtime_path: None,
            runtime_source: None,
            error: None,
        }
    }
}

fn coding_capabilities() -> HarnessCapabilities {
    HarnessCapabilities {
        authentication: true,
        model_selection: true,
        reasoning_effort: true,
        reasoning_stream: true,
        tool_stream: true,
        session_resume: true,
        attachments: true,
        qwenpaw_skills_projection: true,
        qwenpaw_mcp_projection: true,
        provider_skills_discovery: true,
        mcp_tool_allowlist: true,
        ..HarnessCapabilities::default()
    }
}

fn command(name: &'static str, description: &'static str) -> HarnessCommand {
    HarnessCommand {
        name,
        description,
        accepts_arguments: false,
    }
}

fn preset(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    settings: Value,
) -> HarnessApprovalPreset {
    HarnessApprovalPreset {
        id,
        name,
        description,
        settings,
    }
}

/// The complete original directory in original order; no runtime probes.
#[must_use]
pub fn catalog() -> [ProviderCatalogItem; 3] {
    [
        codex(),
        ProviderCatalogItem {
            id: "claude",
            name: "Claude Code",
            coming_soon: true,
            capabilities: HarnessCapabilities::default(),
        },
        qoder(),
    ]
}

#[must_use]
pub fn codex() -> ProviderCatalogItem {
    ProviderCatalogItem {
        id: "codex",
        name: "Codex",
        coming_soon: false,
        capabilities: HarnessCapabilities {
            provider_mcp_discovery: true,
            commands: vec![
                command("compact", "Compact the current Codex thread"),
                command("review", "Review uncommitted workspace changes"),
                command("skills", "List skills available to Codex"),
                command("status", "Show Codex account and session status"),
            ],
            approval_presets: vec![
                preset(
                    "ask",
                    "Ask before changes",
                    "Allow workspace changes and ask before elevated actions.",
                    json!({"sandbox":"workspace-write","approval_policy":"on-request"}),
                ),
                preset(
                    "read-only",
                    "Read only",
                    "Inspect files without changing them.",
                    json!({"sandbox":"read-only","approval_policy":"on-request"}),
                ),
                preset(
                    "workspace",
                    "Workspace access",
                    "Allow workspace changes without confirmation.",
                    json!({"sandbox":"workspace-write","approval_policy":"never"}),
                ),
                preset(
                    "full-access",
                    "Full access",
                    "Allow unrestricted local execution without confirmation.",
                    json!({"sandbox":"danger-full-access","approval_policy":"never"}),
                ),
            ],
            ..coding_capabilities()
        },
    }
}

fn qoder() -> ProviderCatalogItem {
    ProviderCatalogItem {
        id: "qoder",
        name: "Qoder",
        coming_soon: false,
        capabilities: HarnessCapabilities {
            commands: vec![command("compact", "Compact the current Qoder session")],
            approval_presets: vec![
                preset(
                    "ask",
                    "Ask before actions",
                    "Ask before file changes and command execution.",
                    json!({"permission_mode":"default"}),
                ),
                preset(
                    "accept-edits",
                    "Accept edits",
                    "Allow file edits while keeping other safeguards.",
                    json!({"permission_mode":"acceptEdits"}),
                ),
                preset(
                    "plan",
                    "Plan only",
                    "Analyze and plan without changing files.",
                    json!({"permission_mode":"plan"}),
                ),
                preset(
                    "auto",
                    "Automatic",
                    "Let Qoder decide which safe actions can run.",
                    json!({"permission_mode":"auto"}),
                ),
                preset(
                    "full-access",
                    "Full access",
                    "Skip permission checks in a trusted workspace.",
                    json!({"permission_mode":"bypassPermissions"}),
                ),
            ],
            ..coding_capabilities()
        },
    }
}
