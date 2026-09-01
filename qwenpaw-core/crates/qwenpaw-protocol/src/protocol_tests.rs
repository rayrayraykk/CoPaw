use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

#[test]
fn serializes_thread_start_with_camel_case_fields() {
    let response = ThreadStartResponse {
        thread: Thread {
            id: String::from("thread-1"),
            model: String::from("qwen3-coder-plus"),
            workspace_root: Some(String::from("/workspace")),
            status: ThreadStatus::Idle,
            archived: false,
            created_at: 1_700_000_000,
            updated_at: 1_700_000_000,
        },
    };

    assert_eq!(
        serde_json::to_value(response).expect("response should serialize"),
        json!({
            "thread": {
                "id": "thread-1",
                "model": "qwen3-coder-plus",
                "workspaceRoot": "/workspace",
                "status": "idle",
                "archived": false,
                "createdAt": 1_700_000_000,
                "updatedAt": 1_700_000_000
            }
        })
    );
}

#[test]
fn serializes_agent_delta_notification() {
    let event = CoreEvent::AgentMessageDelta(AgentMessageDeltaNotification {
        thread_id: String::from("thread-1"),
        turn_id: String::from("turn-1"),
        item_id: String::from("item-1"),
        delta: String::from("hello"),
    });

    assert_eq!(
        event
            .into_notification()
            .expect("notification should serialize"),
        ServerNotification {
            method: "item/agentMessage/delta",
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "itemId": "item-1",
                "delta": "hello"
            }),
        }
    );
}

#[test]
fn deserializes_structured_user_input() {
    let params: TurnStartParams = serde_json::from_value(json!({
        "threadId": "thread-1",
        "input": [
            {"type": "text", "text": "review this"},
            {
                "type": "fileReference",
                "path": "/workspace/src/lib.rs",
                "startLine": 10,
                "endLine": 20
            }
        ]
    }))
    .expect("params should deserialize");

    assert_eq!(
        params,
        TurnStartParams {
            thread_id: String::from("thread-1"),
            input: vec![
                UserInput::Text {
                    text: String::from("review this"),
                },
                UserInput::FileReference {
                    path: String::from("/workspace/src/lib.rs"),
                    start_line: Some(10),
                    end_line: Some(20),
                },
            ],
        }
    );
}

#[test]
fn serializes_tool_approval_request() {
    let event = CoreEvent::ToolApprovalRequested(ToolApprovalRequestedNotification {
        thread_id: String::from("thread-1"),
        turn_id: String::from("turn-1"),
        approval_id: String::from("approval-1"),
        call_id: String::from("call-1"),
        tool_name: String::from("shell"),
        arguments: String::from("{\"command\":\"cargo test\"}"),
        workspace_root: String::from("/workspace"),
    });

    assert_eq!(
        event
            .into_notification()
            .expect("notification should serialize"),
        ServerNotification {
            method: "tool/approval/requested",
            params: json!({
                "threadId": "thread-1",
                "turnId": "turn-1",
                "approvalId": "approval-1",
                "callId": "call-1",
                "toolName": "shell",
                "arguments": "{\"command\":\"cargo test\"}",
                "workspaceRoot": "/workspace"
            }),
        }
    );
}

#[test]
fn serializes_item_variant_fields_with_camel_case_names() {
    let item = Item::ToolResult {
        id: String::from("item-1"),
        call_id: String::from("call-1"),
        content: String::from("done"),
        is_error: false,
    };

    assert_eq!(
        serde_json::to_value(item).expect("item should serialize"),
        json!({
            "type": "toolResult",
            "id": "item-1",
            "callId": "call-1",
            "content": "done",
            "isError": false
        })
    );
}

#[test]
fn checked_in_contract_artifacts_match_rust_types() {
    assert_eq!(
        normalize_newlines(&typescript_contract()),
        normalize_newlines(include_str!("../../../sdk/typescript/src/protocol.ts"))
    );
    assert_eq!(
        json_schema_contract(),
        serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../docs/api-contract/app-protocol-v2.schema.json"
        ))
        .expect("checked-in schema should be valid JSON")
    );
    assert_eq!(
        app_protocol_fixtures(),
        serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../docs/api-contract/fixtures/app-protocol-v2.json"
        ))
        .expect("checked-in fixtures should be valid JSON")
    );
    assert_eq!(
        normalize_newlines(&protocol_inventory()),
        normalize_newlines(include_str!(
            "../../../docs/api-contract/app-protocol-inventory.md"
        ))
    );
}

fn normalize_newlines(value: &str) -> String {
    value.replace("\r\n", "\n")
}
