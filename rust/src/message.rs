use crate::error::{MultifrostError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const APP_NAME: &str = "comlink_ipc_v3";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Call,
    Response,
    Error,
    Shutdown,
    Ready,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub app: String,
    pub id: String,
    #[serde(rename = "type")]
    pub msg_type: MessageType,
    pub timestamp: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub function: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub args: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub client_name: Option<String>,
}

impl Message {
    pub fn new(msg_type: MessageType) -> Self {
        Self {
            app: APP_NAME.to_string(),
            id: Uuid::new_v4().to_string(),
            msg_type,
            timestamp: current_timestamp(),
            function: None,
            args: None,
            namespace: None,
            result: None,
            error: None,
            client_name: None,
        }
    }

    pub fn create_call(function: &str, args: Vec<serde_json::Value>) -> Self {
        Self {
            app: APP_NAME.to_string(),
            id: Uuid::new_v4().to_string(),
            msg_type: MessageType::Call,
            timestamp: current_timestamp(),
            function: Some(function.to_string()),
            args: Some(args),
            namespace: Some("default".to_string()),
            result: None,
            error: None,
            client_name: None,
        }
    }

    pub fn create_response(result: serde_json::Value, msg_id: &str) -> Self {
        Self {
            app: APP_NAME.to_string(),
            id: msg_id.to_string(),
            msg_type: MessageType::Response,
            timestamp: current_timestamp(),
            function: None,
            args: None,
            namespace: None,
            result: Some(result),
            error: None,
            client_name: None,
        }
    }

    pub fn create_error(error: &str, msg_id: &str) -> Self {
        Self {
            app: APP_NAME.to_string(),
            id: msg_id.to_string(),
            msg_type: MessageType::Error,
            timestamp: current_timestamp(),
            function: None,
            args: None,
            namespace: None,
            result: None,
            error: Some(error.to_string()),
            client_name: None,
        }
    }

    pub fn create_shutdown() -> Self {
        Self::new(MessageType::Shutdown)
    }

    pub fn create_ready() -> Self {
        Self::new(MessageType::Ready)
    }

    pub fn pack(&self) -> Result<Vec<u8>> {
        rmp_serde::to_vec(self).map_err(MultifrostError::from)
    }

    pub fn unpack(data: &[u8]) -> Result<Self> {
        rmp_serde::from_slice(data).map_err(MultifrostError::from)
    }

    pub fn is_valid(&self) -> bool {
        self.app == APP_NAME && !self.id.is_empty()
    }
}

fn current_timestamp() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
