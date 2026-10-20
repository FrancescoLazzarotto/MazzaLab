use rosc::{OscMessage, OscType};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplyStatus {
    Ok,
    Error,
    /// The client is not connected or lacks the permission for this message.
    Denied,
}

/// The JSON reply sent back for a request.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reply {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// The address of the message being answered.
    pub address: String,
    pub status: ReplyStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Reply {
    pub fn ok(address: impl Into<String>, data: Option<serde_json::Value>) -> Self {
        Self {
            workspace_id: None,
            address: address.into(),
            status: ReplyStatus::Ok,
            data,
        }
    }

    pub fn with_status(address: impl Into<String>, status: ReplyStatus) -> Self {
        Self {
            workspace_id: None,
            address: address.into(),
            status,
            data: None,
        }
    }

    pub fn with_workspace(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = Some(workspace_id.into());
        self
    }

    /// `/reply` followed by the original address.
    pub fn reply_address(&self) -> String {
        format!("/reply{}", self.address)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("reply serialization cannot fail")
    }

    pub fn to_message(&self) -> OscMessage {
        OscMessage {
            addr: self.reply_address(),
            args: vec![OscType::String(self.to_json())],
        }
    }
}
