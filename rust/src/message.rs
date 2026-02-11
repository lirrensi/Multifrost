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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_message_creation() {
        let msg = Message::new(MessageType::Call);
        assert_eq!(msg.app, APP_NAME);
        assert_eq!(msg.msg_type, MessageType::Call);
        assert!(!msg.id.is_empty());
        assert!(msg.timestamp > 0.0);
    }

    #[test]
    fn test_create_call_message() {
        let args = vec![json!(5), json!(3)];
        let msg = Message::create_call("add", args.clone());

        assert_eq!(msg.msg_type, MessageType::Call);
        assert_eq!(msg.function, Some("add".to_string()));
        assert_eq!(msg.args, Some(args));
        assert_eq!(msg.namespace, Some("default".to_string()));
        assert!(msg.result.is_none());
        assert!(msg.error.is_none());
    }

    #[test]
    fn test_create_response_message() {
        let result = json!(42);
        let msg_id = "test-id-123";
        let msg = Message::create_response(result.clone(), msg_id);

        assert_eq!(msg.msg_type, MessageType::Response);
        assert_eq!(msg.id, msg_id);
        assert_eq!(msg.result, Some(result));
        assert!(msg.error.is_none());
    }

    #[test]
    fn test_create_error_message() {
        let error = "Something went wrong";
        let msg_id = "test-id-456";
        let msg = Message::create_error(error, msg_id);

        assert_eq!(msg.msg_type, MessageType::Error);
        assert_eq!(msg.id, msg_id);
        assert_eq!(msg.error, Some(error.to_string()));
        assert!(msg.result.is_none());
    }

    #[test]
    fn test_create_shutdown_message() {
        let msg = Message::create_shutdown();
        assert_eq!(msg.msg_type, MessageType::Shutdown);
        assert_eq!(msg.app, APP_NAME);
    }

    #[test]
    fn test_create_ready_message() {
        let msg = Message::create_ready();
        assert_eq!(msg.msg_type, MessageType::Ready);
        assert_eq!(msg.app, APP_NAME);
    }

    #[test]
    fn test_create_stdout_message() {
        let output = "Hello from worker";
        let msg = Message::create_stdout(output);

        assert_eq!(msg.msg_type, MessageType::Stdout);
        assert_eq!(msg.output, Some(output.to_string()));
        assert_eq!(msg.app, APP_NAME);
    }

    #[test]
    fn test_create_stderr_message() {
        let output = "Error occurred";
        let msg = Message::create_stderr(output);

        assert_eq!(msg.msg_type, MessageType::Stderr);
        assert_eq!(msg.output, Some(output.to_string()));
        assert_eq!(msg.app, APP_NAME);
    }

    #[test]
    fn test_create_heartbeat_message() {
        let msg_id = Some("hb-123".to_string());
        let timestamp = 1234567890.0;
        let msg = Message::create_heartbeat(msg_id.clone(), Some(timestamp));

        assert_eq!(msg.msg_type, MessageType::Heartbeat);
        assert_eq!(msg.id, msg_id.unwrap());
        assert!(msg.metadata.is_some());

        let metadata = msg.metadata.unwrap();
        assert_eq!(metadata.get("hb_timestamp"), Some(&json!(timestamp)));
    }

    #[test]
    fn test_create_heartbeat_message_defaults() {
        let msg = Message::create_heartbeat(None, None);

        assert_eq!(msg.msg_type, MessageType::Heartbeat);
        assert!(!msg.id.is_empty());
        assert!(msg.metadata.is_some());

        let metadata = msg.metadata.unwrap();
        assert!(metadata.get("hb_timestamp").is_some());
    }

    #[test]
    fn test_create_heartbeat_response() {
        let request_id = "req-123";
        let original_timestamp = 1234567890.0;
        let msg = Message::create_heartbeat_response(request_id, original_timestamp);

        assert_eq!(msg.msg_type, MessageType::Heartbeat);
        assert_eq!(msg.id, request_id);
        assert!(msg.metadata.is_some());

        let metadata = msg.metadata.unwrap();
        assert_eq!(
            metadata.get("hb_timestamp"),
            Some(&json!(original_timestamp))
        );
        assert_eq!(metadata.get("hb_response"), Some(&json!(true)));
    }

    #[test]
    fn test_message_pack_unpack_roundtrip() {
        let original = Message::create_call("test_func", vec![json!(1), json!(2)]);

        let packed = original.pack().unwrap();
        let unpacked = Message::unpack(&packed).unwrap();

        assert_eq!(original.app, unpacked.app);
        assert_eq!(original.msg_type, unpacked.msg_type);
        assert_eq!(original.function, unpacked.function);
        assert_eq!(original.args, unpacked.args);
        assert_eq!(original.namespace, unpacked.namespace);
    }

    #[test]
    fn test_message_pack_unpack_with_metadata() {
        let metadata = json!({"key": "value", "num": 42});
        let mut msg = Message::create_call("test", vec![]);
        msg.metadata = Some(metadata.clone());

        let packed = msg.pack().unwrap();
        let unpacked = Message::unpack(&packed).unwrap();

        assert_eq!(unpacked.metadata, Some(metadata));
    }

    #[test]
    fn test_message_is_valid() {
        let msg = Message::new(MessageType::Call);
        assert!(msg.is_valid());
    }

    #[test]
    fn test_message_is_valid_wrong_app() {
        let mut msg = Message::new(MessageType::Call);
        msg.app = "wrong_app".to_string();
        assert!(!msg.is_valid());
    }

    #[test]
    fn test_message_is_valid_empty_id() {
        let mut msg = Message::new(MessageType::Call);
        msg.id = String::new();
        assert!(!msg.is_valid());
    }

    #[test]
    fn test_timestamp_serialization_float() {
        let timestamp = 1234567890.123;
        let mut msg = Message::new(MessageType::Call);
        msg.timestamp = timestamp;

        let packed = msg.pack().unwrap();
        let unpacked = Message::unpack(&packed).unwrap();

        assert!((unpacked.timestamp - timestamp).abs() < 0.001);
    }

    #[test]
    fn test_timestamp_serialization_integer() {
        // Test that we can handle integer timestamps from msgpack
        let timestamp = 1234567890.0;
        let mut msg = Message::new(MessageType::Call);
        msg.timestamp = timestamp;

        let packed = msg.pack().unwrap();
        let unpacked = Message::unpack(&packed).unwrap();

        assert!((unpacked.timestamp - timestamp).abs() < 0.001);
    }

    #[test]
    fn test_message_type_equality() {
        assert_eq!(MessageType::Call, MessageType::Call);
        assert_ne!(MessageType::Call, MessageType::Response);
    }

    #[test]
    fn test_message_default() {
        let msg = Message::default();
        assert_eq!(msg.app, APP_NAME);
        assert_eq!(msg.msg_type, MessageType::Call);
        assert!(!msg.id.is_empty());
        assert!(msg.function.is_none());
        assert!(msg.args.is_none());
    }

    #[test]
    fn test_message_with_all_fields() {
        let metadata = json!({"trace_id": "abc123"});
        let metadata_clone = metadata.clone();
        let msg = Message {
            app: APP_NAME.to_string(),
            id: "test-id".to_string(),
            msg_type: MessageType::Call,
            timestamp: 1234567890.0,
            function: Some("test".to_string()),
            args: Some(vec![json!(1)]),
            namespace: Some("test_ns".to_string()),
            result: Some(json!(42)),
            error: None,
            output: None,
            client_name: Some("test_client".to_string()),
            metadata: Some(metadata),
        };

        let packed = msg.pack().unwrap();
        let unpacked = Message::unpack(&packed).unwrap();

        assert_eq!(unpacked.function, Some("test".to_string()));
        assert_eq!(unpacked.namespace, Some("test_ns".to_string()));
        assert_eq!(unpacked.result, Some(json!(42)));
        assert_eq!(unpacked.client_name, Some("test_client".to_string()));
        assert_eq!(unpacked.metadata, Some(metadata_clone));
    }

    #[test]
    fn test_message_clone() {
        let msg = Message::create_call("test", vec![json!(1)]);
        let cloned = msg.clone();

        assert_eq!(msg.id, cloned.id);
        assert_eq!(msg.msg_type, cloned.msg_type);
        assert_eq!(msg.function, cloned.function);
    }

    #[test]
    fn test_message_debug() {
        let msg = Message::create_call("test", vec![json!(1)]);
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("Call"));
        assert!(debug_str.contains("test"));
    }
}
