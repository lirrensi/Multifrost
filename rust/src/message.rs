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
    Stdout,
    Stderr,
    Heartbeat,
    Shutdown,
    Ready,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub app: String,
    pub id: String,
    #[serde(rename = "type")]
    pub msg_type: MessageType,
    #[serde(with = "timestamp_as_f64")]
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
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub client_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Custom serializer for timestamp to handle msgpack integer/float ambiguity
mod timestamp_as_f64 {
    use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(value: &f64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Always serialize as f64 to ensure msgpack uses float type
        value.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Handle both integer and float from msgpack
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Numeric {
            Float(f64),
            Int(i64),
            UInt(u64),
        }

        match Numeric::deserialize(deserializer)? {
            Numeric::Float(f) => Ok(f),
            Numeric::Int(i) => Ok(i as f64),
            Numeric::UInt(u) => Ok(u as f64),
        }
    }
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
            output: None,
            client_name: None,
            metadata: None,
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
            output: None,
            client_name: None,
            metadata: None,
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
            output: None,
            client_name: None,
            metadata: None,
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
            output: None,
            client_name: None,
            metadata: None,
        }
    }

    pub fn create_shutdown() -> Self {
        Self::new(MessageType::Shutdown)
    }

    pub fn create_ready() -> Self {
        Self::new(MessageType::Ready)
    }

    pub fn create_stdout(output: &str) -> Self {
        Self {
            app: APP_NAME.to_string(),
            id: Uuid::new_v4().to_string(),
            msg_type: MessageType::Stdout,
            timestamp: current_timestamp(),
            output: Some(output.to_string()),
            ..Default::default()
        }
    }

    pub fn create_stderr(output: &str) -> Self {
        Self {
            app: APP_NAME.to_string(),
            id: Uuid::new_v4().to_string(),
            msg_type: MessageType::Stderr,
            timestamp: current_timestamp(),
            output: Some(output.to_string()),
            ..Default::default()
        }
    }

    pub fn create_heartbeat(msg_id: Option<String>, timestamp: Option<f64>) -> Self {
        let hb_timestamp = timestamp.unwrap_or_else(|| current_timestamp());
        let metadata = serde_json::json!({
            "hb_timestamp": hb_timestamp
        });

        Self {
            app: APP_NAME.to_string(),
            id: msg_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            msg_type: MessageType::Heartbeat,
            timestamp: current_timestamp(),
            metadata: Some(metadata),
            ..Default::default()
        }
    }

    pub fn create_heartbeat_response(request_id: &str, original_timestamp: f64) -> Self {
        let metadata = serde_json::json!({
            "hb_timestamp": original_timestamp,
            "hb_response": true
        });

        Self {
            app: APP_NAME.to_string(),
            id: request_id.to_string(),
            msg_type: MessageType::Heartbeat,
            timestamp: current_timestamp(),
            metadata: Some(metadata),
            ..Default::default()
        }
    }

    pub fn pack(&self) -> Result<Vec<u8>> {
        // Use named (map-based) serialization so field order doesn't matter
        let packed = rmp_serde::to_vec_named(self).map_err(MultifrostError::from)?;
        Ok(packed)
    }

    pub fn unpack(data: &[u8]) -> Result<Self> {
        rmp_serde::from_slice(data).map_err(MultifrostError::from)
    }

    pub fn is_valid(&self) -> bool {
        self.app == APP_NAME && !self.id.is_empty()
    }
}

impl Default for Message {
    fn default() -> Self {
        Self {
            app: APP_NAME.to_string(),
            id: Uuid::new_v4().to_string(),
            msg_type: MessageType::Call,
            timestamp: current_timestamp(),
            function: None,
            args: None,
            namespace: None,
            result: None,
            error: None,
            output: None,
            client_name: None,
            metadata: None,
        }
    }
}

fn current_timestamp() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
