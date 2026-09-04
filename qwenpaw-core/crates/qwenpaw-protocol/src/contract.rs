use std::collections::BTreeMap;
use std::fmt::Write as _;

use schemars::JsonSchema;
use schemars::schema::RootSchema;
use schemars::schema::Schema;
use serde_json::Value;
use serde_json::json;
use ts_rs::TS;

use crate::AgentMessageDeltaNotification;
use crate::ApprovalDecision;
use crate::ClientInfo;
use crate::ConfigReadParams;
use crate::ConfigReadResponse;
use crate::ConfigWriteParams;
use crate::ConfigWriteResponse;
use crate::CoreConfig;
use crate::ErrorInfo;
use crate::InitializeParams;
use crate::InitializeResponse;
use crate::Item;
use crate::ItemCompletedNotification;
use crate::ItemStartedNotification;
use crate::McpClientInfo;
use crate::McpListParams;
use crate::McpListResponse;
use crate::McpOAuthRevokeParams;
use crate::McpOAuthRevokeResponse;
use crate::McpOAuthStartParams;
use crate::McpOAuthStartResponse;
use crate::McpOAuthStatus;
use crate::McpOAuthStatusParams;
use crate::McpOAuthStatusResponse;
use crate::ModelInfo;
use crate::ModelListParams;
use crate::ModelListResponse;
use crate::ServerInfo;
use crate::Thread;
use crate::ThreadArchiveParams;
use crate::ThreadArchiveResponse;
use crate::ThreadListParams;
use crate::ThreadListResponse;
use crate::ThreadReadParams;
use crate::ThreadReadResponse;
use crate::ThreadResumeParams;
use crate::ThreadResumeResponse;
use crate::ThreadStartParams;
use crate::ThreadStartResponse;
use crate::ThreadStartedNotification;
use crate::ThreadStatus;
use crate::ToolApprovalRequestedNotification;
use crate::ToolApprovalResolvedNotification;
use crate::ToolApprovalRespondParams;
use crate::ToolApprovalRespondResponse;
use crate::Turn;
use crate::TurnCompletedNotification;
use crate::TurnInterruptParams;
use crate::TurnInterruptResponse;
use crate::TurnStartParams;
use crate::TurnStartResponse;
use crate::TurnStartedNotification;
use crate::TurnStatus;
use crate::UserInput;
use crate::WorkspaceInfo;
use crate::WorkspaceListParams;
use crate::WorkspaceListResponse;
use crate::WorkspaceReadParams;
use crate::WorkspaceReadResponse;

pub const REQUEST_METHODS: &[(&str, &str, &str)] = &[
    ("initialize", "InitializeParams", "InitializeResponse"),
    ("thread/start", "ThreadStartParams", "ThreadStartResponse"),
    (
        "thread/resume",
        "ThreadResumeParams",
        "ThreadResumeResponse",
    ),
    (
        "thread/archive",
        "ThreadArchiveParams",
        "ThreadArchiveResponse",
    ),
    ("thread/list", "ThreadListParams", "ThreadListResponse"),
    ("thread/read", "ThreadReadParams", "ThreadReadResponse"),
    ("turn/start", "TurnStartParams", "TurnStartResponse"),
    (
        "turn/interrupt",
        "TurnInterruptParams",
        "TurnInterruptResponse",
    ),
    (
        "tool/approval/respond",
        "ToolApprovalRespondParams",
        "ToolApprovalRespondResponse",
    ),
    ("model/list", "ModelListParams", "ModelListResponse"),
    ("config/read", "ConfigReadParams", "ConfigReadResponse"),
    ("config/write", "ConfigWriteParams", "ConfigWriteResponse"),
    (
        "workspace/list",
        "WorkspaceListParams",
        "WorkspaceListResponse",
    ),
    (
        "workspace/read",
        "WorkspaceReadParams",
        "WorkspaceReadResponse",
    ),
    ("mcp/list", "McpListParams", "McpListResponse"),
    (
        "mcp/oauth/start",
        "McpOAuthStartParams",
        "McpOAuthStartResponse",
    ),
    (
        "mcp/oauth/status",
        "McpOAuthStatusParams",
        "McpOAuthStatusResponse",
    ),
    (
        "mcp/oauth/revoke",
        "McpOAuthRevokeParams",
        "McpOAuthRevokeResponse",
    ),
];

pub const SERVER_NOTIFICATIONS: &[(&str, &str)] = &[
    ("thread/started", "ThreadStartedNotification"),
    ("turn/started", "TurnStartedNotification"),
    ("item/started", "ItemStartedNotification"),
    ("item/agentMessage/delta", "AgentMessageDeltaNotification"),
    ("item/completed", "ItemCompletedNotification"),
    (
        "tool/approval/requested",
        "ToolApprovalRequestedNotification",
    ),
    ("tool/approval/resolved", "ToolApprovalResolvedNotification"),
    ("turn/completed", "TurnCompletedNotification"),
];

/// Returns the canonical TypeScript declarations for the current App Protocol.
#[must_use]
pub fn typescript_contract() -> String {
    let declarations = [
        ClientInfo::decl(),
        InitializeParams::decl(),
        ServerInfo::decl(),
        InitializeResponse::decl(),
        ThreadStatus::decl(),
        Thread::decl(),
        ThreadStartParams::decl(),
        ThreadStartResponse::decl(),
        ThreadResumeParams::decl(),
        ThreadResumeResponse::decl(),
        ThreadArchiveParams::decl(),
        ThreadArchiveResponse::decl(),
        ThreadListParams::decl(),
        ThreadListResponse::decl(),
        ThreadReadParams::decl(),
        Item::decl(),
        ErrorInfo::decl(),
        TurnStatus::decl(),
        Turn::decl(),
        ThreadReadResponse::decl(),
        UserInput::decl(),
        TurnStartParams::decl(),
        TurnStartResponse::decl(),
        TurnInterruptParams::decl(),
        TurnInterruptResponse::decl(),
        ApprovalDecision::decl(),
        ToolApprovalRespondParams::decl(),
        ToolApprovalRespondResponse::decl(),
        ModelListParams::decl(),
        ModelInfo::decl(),
        ModelListResponse::decl(),
        ConfigReadParams::decl(),
        ConfigReadResponse::decl(),
        ConfigWriteParams::decl(),
        ConfigWriteResponse::decl(),
        CoreConfig::decl(),
        WorkspaceListParams::decl(),
        WorkspaceListResponse::decl(),
        WorkspaceReadParams::decl(),
        WorkspaceReadResponse::decl(),
        WorkspaceInfo::decl(),
        McpListParams::decl(),
        McpListResponse::decl(),
        McpClientInfo::decl(),
        McpOAuthStatus::decl(),
        McpOAuthStartParams::decl(),
        McpOAuthStartResponse::decl(),
        McpOAuthStatusParams::decl(),
        McpOAuthStatusResponse::decl(),
        McpOAuthRevokeParams::decl(),
        McpOAuthRevokeResponse::decl(),
        ThreadStartedNotification::decl(),
        TurnStartedNotification::decl(),
        ItemStartedNotification::decl(),
        AgentMessageDeltaNotification::decl(),
        ItemCompletedNotification::decl(),
        TurnCompletedNotification::decl(),
        ToolApprovalRequestedNotification::decl(),
        ToolApprovalResolvedNotification::decl(),
    ];
    let mut output = String::from("// Generated by qwenpaw-protocol. Do not edit.\n\n");
    for declaration in declarations {
        output.push_str(&export_declaration(&declaration));
        output.push_str("\n\n");
    }
    output.push_str("export interface AppProtocolRequests {\n");
    for (method, params, response) in REQUEST_METHODS {
        writeln!(
            output,
            "  readonly \"{method}\": {{ readonly params: {params}; readonly result: {response} }};"
        )
        .expect("writing to a String should succeed");
    }
    output.push_str("}\n\nexport interface AppProtocolServerNotifications {\n");
    for (method, payload) in SERVER_NOTIFICATIONS {
        writeln!(output, "  readonly \"{method}\": {payload};")
            .expect("writing to a String should succeed");
    }
    writeln!(
        output,
        "}}\n\nexport const PROTOCOL_VERSION = {} as const;",
        crate::PROTOCOL_VERSION
    )
    .expect("writing to a String should succeed");
    output.push_str("\n\nexport const APP_PROTOCOL_REQUEST_METHODS = [\n");
    for (method, _, _) in REQUEST_METHODS {
        writeln!(output, "  \"{method}\",").expect("writing to a String should succeed");
    }
    output.push_str("] as const;\n\nexport const APP_PROTOCOL_SERVER_NOTIFICATION_METHODS = [\n");
    for (method, _) in SERVER_NOTIFICATIONS {
        writeln!(output, "  \"{method}\",").expect("writing to a String should succeed");
    }
    output.push_str("] as const;");
    output
}

/// Returns the generated Python constants for the current App Protocol.
#[must_use]
pub fn python_contract() -> String {
    let mut output =
        String::from("# Generated by qwenpaw-protocol. Do not edit.\n\nPROTOCOL_VERSION = ");
    writeln!(output, "{}\n", crate::PROTOCOL_VERSION).expect("writing to a String should succeed");
    output.push_str("APP_PROTOCOL_REQUEST_METHODS = (\n");
    for (method, _, _) in REQUEST_METHODS {
        writeln!(output, "    \"{method}\",").expect("writing to a String should succeed");
    }
    output.push_str(")\n\nAPP_PROTOCOL_SERVER_NOTIFICATION_METHODS = (\n");
    for (method, _) in SERVER_NOTIFICATIONS {
        writeln!(output, "    \"{method}\",").expect("writing to a String should succeed");
    }
    output.push_str(")\n");
    output
}

/// Returns a JSON Schema registry for every current App Protocol payload.
#[must_use]
pub fn json_schema_contract() -> Value {
    let mut definitions = BTreeMap::new();
    add_schema::<ClientInfo>(&mut definitions);
    add_schema::<InitializeParams>(&mut definitions);
    add_schema::<ServerInfo>(&mut definitions);
    add_schema::<InitializeResponse>(&mut definitions);
    add_schema::<ThreadStartParams>(&mut definitions);
    add_schema::<ThreadStartResponse>(&mut definitions);
    add_schema::<ThreadResumeParams>(&mut definitions);
    add_schema::<ThreadResumeResponse>(&mut definitions);
    add_schema::<ThreadArchiveParams>(&mut definitions);
    add_schema::<ThreadArchiveResponse>(&mut definitions);
    add_schema::<ThreadListParams>(&mut definitions);
    add_schema::<ThreadListResponse>(&mut definitions);
    add_schema::<ThreadReadParams>(&mut definitions);
    add_schema::<ThreadReadResponse>(&mut definitions);
    add_schema::<TurnStartParams>(&mut definitions);
    add_schema::<TurnStartResponse>(&mut definitions);
    add_schema::<TurnInterruptParams>(&mut definitions);
    add_schema::<TurnInterruptResponse>(&mut definitions);
    add_schema::<ToolApprovalRespondParams>(&mut definitions);
    add_schema::<ToolApprovalRespondResponse>(&mut definitions);
    add_schema::<ModelListParams>(&mut definitions);
    add_schema::<ModelListResponse>(&mut definitions);
    add_schema::<ConfigReadParams>(&mut definitions);
    add_schema::<ConfigReadResponse>(&mut definitions);
    add_schema::<ConfigWriteParams>(&mut definitions);
    add_schema::<ConfigWriteResponse>(&mut definitions);
    add_schema::<WorkspaceListParams>(&mut definitions);
    add_schema::<WorkspaceListResponse>(&mut definitions);
    add_schema::<WorkspaceReadParams>(&mut definitions);
    add_schema::<WorkspaceReadResponse>(&mut definitions);
    add_schema::<McpListParams>(&mut definitions);
    add_schema::<McpListResponse>(&mut definitions);
    add_schema::<McpOAuthStartParams>(&mut definitions);
    add_schema::<McpOAuthStartResponse>(&mut definitions);
    add_schema::<McpOAuthStatusParams>(&mut definitions);
    add_schema::<McpOAuthStatusResponse>(&mut definitions);
    add_schema::<McpOAuthRevokeParams>(&mut definitions);
    add_schema::<McpOAuthRevokeResponse>(&mut definitions);
    add_schema::<ThreadStartedNotification>(&mut definitions);
    add_schema::<TurnStartedNotification>(&mut definitions);
    add_schema::<ItemStartedNotification>(&mut definitions);
    add_schema::<AgentMessageDeltaNotification>(&mut definitions);
    add_schema::<ItemCompletedNotification>(&mut definitions);
    add_schema::<TurnCompletedNotification>(&mut definitions);
    add_schema::<ToolApprovalRequestedNotification>(&mut definitions);
    add_schema::<ToolApprovalResolvedNotification>(&mut definitions);

    let requests = REQUEST_METHODS
        .iter()
        .map(|(method, params, response)| {
            (
                *method,
                json!({
                    "params": {"$ref": format!("#/definitions/{params}")},
                    "result": {"$ref": format!("#/definitions/{response}")}
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let notifications = SERVER_NOTIFICATIONS
        .iter()
        .map(|(method, payload)| (*method, json!({"$ref": format!("#/definitions/{payload}")})))
        .collect::<BTreeMap<_, _>>();

    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": format!("QwenPaw App Protocol v{}", crate::PROTOCOL_VERSION),
        "protocolVersion": crate::PROTOCOL_VERSION,
        "clientNotifications": {
            "initialized": {"type": "object", "additionalProperties": false}
        },
        "requests": requests,
        "serverNotifications": notifications,
        "definitions": definitions
    })
}

/// Returns typed wire examples used by consumers and compatibility tests.
#[must_use]
pub fn app_protocol_fixtures() -> Value {
    let thread = sample_thread();
    let turn = sample_turn();
    json!({
        "protocolVersion": crate::PROTOCOL_VERSION,
        "requests": sample_requests(&thread, &turn),
        "responses": sample_responses(&thread, &turn),
        "serverNotifications": sample_notifications(&thread, turn),
    })
}

fn sample_requests(thread: &Thread, turn: &Turn) -> Value {
    json!({
        "initialize": InitializeParams {
            client_info: ClientInfo {
                name: String::from("qwenpaw_vscode"), version: String::from("0.1.0"),
                title: Some(String::from("QwenPaw VS Code Extension")),
            },
        },
        "thread/start": ThreadStartParams {
            model: Some(String::from("qwen3-coder-plus")),
            workspace_root: Some(String::from("/workspace")),
        },
        "thread/resume": ThreadResumeParams { thread_id: thread.id.clone() },
        "thread/archive": ThreadArchiveParams { thread_id: thread.id.clone() },
        "thread/list": ThreadListParams::default(),
        "thread/read": ThreadReadParams { thread_id: thread.id.clone() },
        "turn/start": TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![
                UserInput::Text { text: String::from("review this file") },
                UserInput::FileReference {
                    path: String::from("/workspace/src/lib.rs"),
                    start_line: Some(10), end_line: Some(20),
                },
            ],
        },
        "turn/interrupt": TurnInterruptParams {
            thread_id: thread.id.clone(), turn_id: turn.id.clone(),
        },
        "tool/approval/respond": ToolApprovalRespondParams {
            approval_id: String::from("approval-1"), decision: ApprovalDecision::Approved,
        },
        "model/list": ModelListParams::default(),
        "config/read": ConfigReadParams::default(),
        "config/write": ConfigWriteParams {
            base_url: Some(String::from("https://example.test/v1")),
            default_model: Some(String::from("qwen3-coder-plus")),
        },
        "workspace/list": WorkspaceListParams::default(),
        "workspace/read": WorkspaceReadParams { root: String::from("/workspace") },
        "mcp/list": McpListParams::default(),
        "mcp/oauth/start": McpOAuthStartParams {
            server_id: String::from("remote-tools"),
            ..McpOAuthStartParams::default()
        },
        "mcp/oauth/status": McpOAuthStatusParams {
            server_id: String::from("remote-tools"),
        },
        "mcp/oauth/revoke": McpOAuthRevokeParams {
            server_id: String::from("remote-tools"),
        },
    })
}

fn sample_responses(thread: &Thread, turn: &Turn) -> Value {
    json!({
        "initialize": InitializeResponse {
            protocol_version: crate::PROTOCOL_VERSION,
            server_info: ServerInfo {
                name: String::from("qwenpaw-core"), version: String::from(env!("CARGO_PKG_VERSION")),
            },
        },
        "thread/start": ThreadStartResponse { thread: thread.clone() },
        "thread/resume": ThreadResumeResponse { thread: thread.clone() },
        "thread/archive": ThreadArchiveResponse {
            thread: Thread { archived: true, ..thread.clone() },
        },
        "thread/list": ThreadListResponse { data: vec![thread.clone()], next_cursor: None },
        "thread/read": ThreadReadResponse {
            thread: thread.clone(), turns: vec![turn.clone()],
        },
        "turn/start": TurnStartResponse { turn: turn.clone() },
        "turn/interrupt": TurnInterruptResponse { accepted: true },
        "tool/approval/respond": ToolApprovalRespondResponse { accepted: true },
        "model/list": ModelListResponse {
            data: vec![ModelInfo {
                id: String::from("qwen3-coder-plus"),
                display_name: String::from("Qwen3 Coder Plus"), is_default: true,
            }],
        },
        "config/read": ConfigReadResponse { config: sample_config() },
        "config/write": ConfigWriteResponse { config: sample_config() },
        "workspace/list": WorkspaceListResponse { data: vec![sample_workspace()] },
        "workspace/read": WorkspaceReadResponse { workspace: sample_workspace() },
        "mcp/list": McpListResponse {
            data: vec![sample_mcp_client()],
        },
        "mcp/oauth/start": McpOAuthStartResponse {
            authorization_url: String::from("https://auth.example.test/authorize"),
            session_id: String::from("opaque-session"),
        },
        "mcp/oauth/status": McpOAuthStatusResponse {
            status: sample_oauth_status(),
        },
        "mcp/oauth/revoke": McpOAuthRevokeResponse { revoked: true },
    })
}

fn sample_oauth_status() -> McpOAuthStatus {
    McpOAuthStatus {
        authorized: true,
        expires_at: 1_700_003_600.0,
        scope: String::from("tools.read"),
        client_id: String::from("qwenpaw-native"),
    }
}

fn sample_mcp_client() -> McpClientInfo {
    McpClientInfo {
        server_id: String::from("remote-tools"),
        name: String::from("Remote Tools"),
        description: String::from("Example remote MCP server"),
        enabled: true,
        transport: String::from("streamable_http"),
        url: String::from("https://mcp.example.test"),
        oauth_status: Some(sample_oauth_status()),
    }
}

fn sample_notifications(thread: &Thread, turn: Turn) -> Value {
    json!({
        "thread/started": ThreadStartedNotification { thread: thread.clone() },
        "turn/started": TurnStartedNotification { turn: turn.clone() },
        "item/started": ItemStartedNotification {
            thread_id: thread.id.clone(), turn_id: turn.id.clone(), item: turn.items[0].clone(),
        },
        "item/agentMessage/delta": AgentMessageDeltaNotification {
            thread_id: thread.id.clone(), turn_id: turn.id.clone(),
            item_id: String::from("item-agent-1"), delta: String::from("hello"),
        },
        "item/completed": ItemCompletedNotification {
            thread_id: thread.id.clone(), turn_id: turn.id.clone(), item: turn.items[0].clone(),
        },
        "tool/approval/requested": ToolApprovalRequestedNotification {
            thread_id: thread.id.clone(), turn_id: turn.id.clone(),
            approval_id: String::from("approval-1"), call_id: String::from("call-1"),
            tool_name: String::from("shell"),
            arguments: String::from("{\"command\":\"cargo test\"}"),
            workspace_root: String::from("/workspace"),
        },
        "tool/approval/resolved": ToolApprovalResolvedNotification {
            thread_id: thread.id.clone(), turn_id: turn.id.clone(),
            approval_id: String::from("approval-1"), decision: ApprovalDecision::Approved,
        },
        "turn/completed": TurnCompletedNotification { turn },
    })
}

/// Returns a human-readable inventory generated from the same method tables.
#[must_use]
pub fn protocol_inventory() -> String {
    let mut output = format!(
        "# App Protocol v{} inventory\n\nThis file is generated from `qwenpaw-protocol`.\n\n## Requests\n\n| Method | Params | Result |\n| --- | --- | --- |\n",
        crate::PROTOCOL_VERSION
    );
    for (method, params, response) in REQUEST_METHODS {
        writeln!(output, "| `{method}` | `{params}` | `{response}` |")
            .expect("writing to a String should succeed");
    }
    output.push_str("\n## Client notifications\n\n- `initialized`\n\n## Server notifications\n\n| Method | Payload |\n| --- | --- |\n");
    for (method, payload) in SERVER_NOTIFICATIONS {
        writeln!(output, "| `{method}` | `{payload}` |")
            .expect("writing to a String should succeed");
    }
    output
}

fn add_schema<T: JsonSchema>(definitions: &mut BTreeMap<String, Schema>) {
    let RootSchema {
        schema,
        definitions: nested,
        ..
    } = schemars::schema_for!(T);
    definitions.extend(nested);
    definitions.insert(T::schema_name(), Schema::Object(schema));
}

fn export_declaration(declaration: &str) -> String {
    if declaration.starts_with("type ") {
        declaration.replacen("type ", "export type ", 1)
    } else if declaration.starts_with("interface ") {
        declaration.replacen("interface ", "export interface ", 1)
    } else {
        declaration.to_owned()
    }
}

fn sample_thread() -> Thread {
    Thread {
        id: String::from("thread-1"),
        model: String::from("qwen3-coder-plus"),
        workspace_root: Some(String::from("/workspace")),
        status: ThreadStatus::Idle,
        archived: false,
        created_at: 1_700_000_000,
        updated_at: 1_700_000_000,
    }
}

fn sample_turn() -> Turn {
    Turn {
        id: String::from("turn-1"),
        thread_id: String::from("thread-1"),
        status: TurnStatus::Completed,
        items: vec![Item::AgentMessage {
            id: String::from("item-agent-1"),
            text: String::from("hello"),
        }],
        error: None,
    }
}

fn sample_config() -> CoreConfig {
    CoreConfig {
        base_url: String::from("https://example.test/v1"),
        default_model: String::from("qwen3-coder-plus"),
        api_key_configured: true,
    }
}

fn sample_workspace() -> WorkspaceInfo {
    WorkspaceInfo {
        root: String::from("/workspace"),
        thread_count: 2,
        archived_thread_count: 1,
        updated_at: 1_700_000_000,
    }
}
