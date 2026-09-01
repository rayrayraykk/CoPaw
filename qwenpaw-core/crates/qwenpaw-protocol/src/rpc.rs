use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClientMessage {
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SuccessResponse {
    pub id: Value,
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ErrorResponse {
    pub id: Value,
    pub error: RpcError,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum ServerResponse {
    Success(SuccessResponse),
    Error(ErrorResponse),
}

impl ServerResponse {
    #[must_use]
    pub fn success(id: Value, result: Value) -> Self {
        Self::Success(SuccessResponse { id, result })
    }

    #[must_use]
    pub fn error(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self::Error(ErrorResponse {
            id,
            error: RpcError {
                code,
                message: message.into(),
            },
        })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ServerNotification {
    pub method: &'static str,
    pub params: Value,
}
