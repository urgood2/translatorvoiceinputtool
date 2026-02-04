//! JSON-RPC 2.0 message types for IPC communication.

#![allow(dead_code)] // Types will be used when RpcClient is integrated

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// JSON-RPC 2.0 request ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(u64),
    String(String),
}

impl From<u64> for RequestId {
    fn from(id: u64) -> Self {
        RequestId::Number(id)
    }
}

impl From<String> for RequestId {
    fn from(id: String) -> Self {
        RequestId::String(id)
    }
}

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize)]
pub struct Request {
    pub jsonrpc: &'static str,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Request {
    /// Create a new request with the given method and parameters.
    pub fn new(id: impl Into<RequestId>, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id: id.into(),
            method: method.into(),
            params,
        }
    }
}

/// JSON-RPC 2.0 error data with our custom kind field.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorData {
    pub kind: String,
    #[serde(default)]
    pub details: Option<Value>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Deserialize)]
pub struct RpcErrorObject {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub data: Option<ErrorData>,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Option<RequestId>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<RpcErrorObject>,
}

impl Response {
    /// Check if this is a successful response.
    pub fn is_success(&self) -> bool {
        self.error.is_none() && self.result.is_some()
    }

    /// Get the error kind string if this is an error response.
    pub fn error_kind(&self) -> Option<&str> {
        self.error.as_ref()?.data.as_ref()?.kind.as_str().into()
    }
}

/// JSON-RPC 2.0 notification (no id).
#[derive(Debug, Clone, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Incoming message that could be either a response or notification.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum IncomingMessage {
    Response(Response),
    Notification(Notification),
}

impl IncomingMessage {
    /// Check if this message has an ID (i.e., is a response, not a notification).
    pub fn has_id(&self) -> bool {
        match self {
            IncomingMessage::Response(r) => r.id.is_some(),
            IncomingMessage::Notification(_) => false,
        }
    }
}

/// Timeout configuration for RPC methods.
pub struct TimeoutConfig;

impl TimeoutConfig {
    /// Get the timeout duration for a method.
    pub fn get(method: &str) -> Duration {
        use phf::phf_map;

        static TIMEOUTS: phf::Map<&'static str, u64> = phf_map! {
            "system.ping" => 1,
            "system.info" => 2,
            "system.shutdown" => 2,
            "audio.list_devices" => 2,
            "audio.set_device" => 2,
            "audio.meter_start" => 2,
            "audio.meter_stop" => 2,
            "model.get_status" => 2,
            "model.purge_cache" => 10,
            "asr.initialize" => 1200, // 20 minutes for first-run download
            "recording.start" => 2,
            "recording.stop" => 2,
            "recording.cancel" => 2,
            "replacements.set_rules" => 2,
            "status.get" => 2,
        };

        let secs = TIMEOUTS.get(method).copied().unwrap_or(5);
        Duration::from_secs(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = Request::new(1u64, "system.ping", None);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"system.ping\""));
    }

    #[test]
    fn test_timeout_config() {
        assert_eq!(TimeoutConfig::get("system.ping"), Duration::from_secs(1));
        assert_eq!(TimeoutConfig::get("asr.initialize"), Duration::from_secs(1200));
        assert_eq!(TimeoutConfig::get("unknown.method"), Duration::from_secs(5));
    }

    #[test]
    fn test_response_parsing() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"version":"0.1.0"}}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert!(resp.is_success());
        assert!(resp.id.is_some());
    }

    #[test]
    fn test_error_response_parsing() {
        let json = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found","data":{"kind":"E_METHOD_NOT_FOUND"}}}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert!(!resp.is_success());
        assert_eq!(resp.error_kind(), Some("E_METHOD_NOT_FOUND"));
    }

    #[test]
    fn test_notification_parsing() {
        let json = r#"{"jsonrpc":"2.0","method":"event.status_changed","params":{"state":"idle"}}"#;
        let notif: Notification = serde_json::from_str(json).unwrap();
        assert_eq!(notif.method, "event.status_changed");
    }
}
