use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Default socket path: `$GLIMPSED_SOCKET` or `$XDG_RUNTIME_DIR/glimpsed.sock`.
pub fn socket_path() -> std::io::Result<PathBuf> {
    if let Ok(path) = std::env::var("GLIMPSED_SOCKET") {
        return Ok(PathBuf::from(path));
    }
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set")
    })?;
    Ok(PathBuf::from(runtime_dir).join("glimpsed.sock"))
}

/// Wire message from client to daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub body: RequestBody,
}

/// Client → Daemon request payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RequestBody {
    /// One-shot read of current state for a topic.
    Get { topic: String },
    /// Subscribe to live updates matching a wildcard pattern.
    Subscribe { pattern: String },
    /// Remove a subscription.
    Unsubscribe { pattern: String },
    /// Invoke a provider method.
    Call {
        method: String,
        params: serde_json::Value,
    },
}

/// Wire message from daemon to client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(flatten)]
    pub body: ResponseBody,
}

/// Daemon → Client response payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ResponseBody {
    /// Reply to a Get request.
    GetResult {
        topic: String,
        #[serde(flatten)]
        result: RequestResult,
    },
    /// Acknowledgement of a Subscribe request.
    SubscribeAck {
        pattern: String,
        available: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Acknowledgement of an Unsubscribe request.
    UnsubscribeAck { pattern: String },
    /// Reply to a Call request.
    CallResult {
        method: String,
        #[serde(flatten)]
        result: RequestResult,
    },
    /// Live event from a subscription.
    Event {
        topic: String,
        /// Milliseconds since Unix epoch.
        ts: u64,
        data: serde_json::Value,
    },
    /// A provider became unavailable.
    ProviderUnavailable { provider: String, error: String },
}

/// Current time in milliseconds since Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Success or error payload for Get and Call responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RequestResult {
    Ok { data: serde_json::Value },
    Error { code: u32, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn roundtrip_request(req: &Request, expected_json: serde_json::Value) {
        let serialized = serde_json::to_value(req).unwrap();
        assert_eq!(serialized, expected_json);
        let deserialized: Request = serde_json::from_value(serialized).unwrap();
        assert_eq!(&deserialized, req);

        let json_str = serde_json::to_string(req).unwrap();
        let from_str: Request = serde_json::from_str(&json_str).unwrap();
        assert_eq!(&from_str, req);
    }

    fn roundtrip_response(resp: &Response, expected_json: serde_json::Value) {
        let serialized = serde_json::to_value(resp).unwrap();
        assert_eq!(serialized, expected_json);
        let deserialized: Response = serde_json::from_value(serialized).unwrap();
        assert_eq!(&deserialized, resp);

        let json_str = serde_json::to_string(resp).unwrap();
        let from_str: Response = serde_json::from_str(&json_str).unwrap();
        assert_eq!(&from_str, resp);
    }

    #[test]
    fn request_get() {
        roundtrip_request(
            &Request {
                id: 1,
                body: RequestBody::Get {
                    topic: "battery.status".into(),
                },
            },
            json!({"id": 1, "type": "get", "data": {"topic": "battery.status"}}),
        );
    }

    #[test]
    fn request_subscribe() {
        roundtrip_request(
            &Request {
                id: 2,
                body: RequestBody::Subscribe {
                    pattern: "audio.**".into(),
                },
            },
            json!({"id": 2, "type": "subscribe", "data": {"pattern": "audio.**"}}),
        );
    }

    #[test]
    fn request_unsubscribe() {
        roundtrip_request(
            &Request {
                id: 3,
                body: RequestBody::Unsubscribe {
                    pattern: "audio.**".into(),
                },
            },
            json!({"id": 3, "type": "unsubscribe", "data": {"pattern": "audio.**"}}),
        );
    }

    #[test]
    fn request_call() {
        roundtrip_request(
            &Request {
                id: 4,
                body: RequestBody::Call {
                    method: "audio.set_volume".into(),
                    params: json!({"node_id": 48, "volume": 0.5}),
                },
            },
            json!({"id": 4, "type": "call", "data": {"method": "audio.set_volume", "params": {"node_id": 48, "volume": 0.5}}}),
        );
    }

    #[test]
    fn response_subscribe_ack() {
        roundtrip_response(
            &Response {
                id: 2,
                body: ResponseBody::SubscribeAck {
                    pattern: "battery.**".into(),
                    available: true,
                    error: None,
                },
            },
            json!({"id": 2, "type": "subscribe_ack", "data": {"pattern": "battery.**", "available": true}}),
        );
    }

    #[test]
    fn response_event() {
        roundtrip_response(
            &Response {
                id: 2,
                body: ResponseBody::Event {
                    topic: "battery.status".into(),
                    ts: 1700000000000,
                    data: json!({"percentage": 85}),
                },
            },
            json!({"id": 2, "type": "event", "data": {"topic": "battery.status", "ts": 1700000000000u64, "data": {"percentage": 85}}}),
        );
    }

    #[test]
    fn response_get_result_ok() {
        roundtrip_response(
            &Response {
                id: 1,
                body: ResponseBody::GetResult {
                    topic: "battery.status".into(),
                    result: RequestResult::Ok {
                        data: json!({"percentage": 85}),
                    },
                },
            },
            json!({"id": 1, "type": "get_result", "data": {"topic": "battery.status", "status": "ok", "data": {"percentage": 85}}}),
        );
    }

    #[test]
    fn response_get_result_error() {
        roundtrip_response(
            &Response {
                id: 1,
                body: ResponseBody::GetResult {
                    topic: "bluetooth.devices".into(),
                    result: RequestResult::Error {
                        code: 1,
                        message: "provider unavailable".into(),
                    },
                },
            },
            json!({"id": 1, "type": "get_result", "data": {"topic": "bluetooth.devices", "status": "error", "code": 1, "message": "provider unavailable"}}),
        );
    }

    #[test]
    fn response_call_result_ok() {
        roundtrip_response(
            &Response {
                id: 4,
                body: ResponseBody::CallResult {
                    method: "audio.set_volume".into(),
                    result: RequestResult::Ok { data: json!(null) },
                },
            },
            json!({"id": 4, "type": "call_result", "data": {"method": "audio.set_volume", "status": "ok", "data": null}}),
        );
    }

    #[test]
    fn response_call_result_error() {
        roundtrip_response(
            &Response {
                id: 4,
                body: ResponseBody::CallResult {
                    method: "audio.set_volume".into(),
                    result: RequestResult::Error {
                        code: 4,
                        message: "invalid parameters".into(),
                    },
                },
            },
            json!({"id": 4, "type": "call_result", "data": {"method": "audio.set_volume", "status": "error", "code": 4, "message": "invalid parameters"}}),
        );
    }

    #[test]
    fn response_provider_unavailable() {
        roundtrip_response(
            &Response {
                id: 2,
                body: ResponseBody::ProviderUnavailable {
                    provider: "bluetooth".into(),
                    error: "BlueZ not running".into(),
                },
            },
            json!({"id": 2, "type": "provider_unavailable", "data": {"provider": "bluetooth", "error": "BlueZ not running"}}),
        );
    }

    #[test]
    fn ndjson_line_has_no_raw_newlines() {
        let event = Response {
            id: 1,
            body: ResponseBody::Event {
                topic: "test".into(),
                ts: 0,
                data: json!({"text": "line1\nline2"}),
            },
        };
        let line = serde_json::to_string(&event).unwrap();
        assert!(!line.contains('\n'));
    }
}
