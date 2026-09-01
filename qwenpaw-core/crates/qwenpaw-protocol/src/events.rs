use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

use crate::ApprovalDecision;
use crate::Item;
use crate::ServerNotification;
use crate::Thread;
use crate::Turn;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThreadStartedNotification {
    pub thread: Thread,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartedNotification {
    pub turn: Turn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ItemStartedNotification {
    pub thread_id: String,
    pub turn_id: String,
    pub item: Item,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageDeltaNotification {
    pub thread_id: String,
    pub turn_id: String,
    pub item_id: String,
    pub delta: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ItemCompletedNotification {
    pub thread_id: String,
    pub turn_id: String,
    pub item: Item,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct TurnCompletedNotification {
    pub turn: Turn,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalRequestedNotification {
    pub thread_id: String,
    pub turn_id: String,
    pub approval_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub arguments: String,
    pub workspace_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalResolvedNotification {
    pub thread_id: String,
    pub turn_id: String,
    pub approval_id: String,
    pub decision: ApprovalDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    TurnStarted(TurnStartedNotification),
    ItemStarted(ItemStartedNotification),
    AgentMessageDelta(AgentMessageDeltaNotification),
    ItemCompleted(ItemCompletedNotification),
    ToolApprovalRequested(ToolApprovalRequestedNotification),
    ToolApprovalResolved(ToolApprovalResolvedNotification),
    TurnCompleted(TurnCompletedNotification),
}

impl CoreEvent {
    /// Converts a typed core event into its wire notification.
    ///
    /// # Errors
    ///
    /// Returns an error when the typed notification payload cannot be
    /// serialized to JSON.
    pub fn into_notification(self) -> Result<ServerNotification, serde_json::Error> {
        let (method, params) = match self {
            Self::TurnStarted(params) => ("turn/started", serde_json::to_value(params)?),
            Self::ItemStarted(params) => ("item/started", serde_json::to_value(params)?),
            Self::AgentMessageDelta(params) => {
                ("item/agentMessage/delta", serde_json::to_value(params)?)
            }
            Self::ItemCompleted(params) => ("item/completed", serde_json::to_value(params)?),
            Self::ToolApprovalRequested(params) => {
                ("tool/approval/requested", serde_json::to_value(params)?)
            }
            Self::ToolApprovalResolved(params) => {
                ("tool/approval/resolved", serde_json::to_value(params)?)
            }
            Self::TurnCompleted(params) => ("turn/completed", serde_json::to_value(params)?),
        };
        Ok(ServerNotification { method, params })
    }
}
