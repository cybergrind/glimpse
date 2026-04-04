use serde::{Deserialize, Serialize};

/// Client → Daemon request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Request {
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

/// Daemon → Client response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Response {
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
        data: serde_json::Value,
    },
    /// A provider became unavailable.
    ProviderUnavailable { provider: String, error: String },
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
            &Request::Get {
                topic: "battery.status".into(),
            },
            json!({"type": "get", "data": {"topic": "battery.status"}}),
        );
    }

    #[test]
    fn request_subscribe() {
        roundtrip_request(
            &Request::Subscribe {
                pattern: "audio.**".into(),
            },
            json!({"type": "subscribe", "data": {"pattern": "audio.**"}}),
        );
    }

    #[test]
    fn request_unsubscribe() {
        roundtrip_request(
            &Request::Unsubscribe {
                pattern: "audio.**".into(),
            },
            json!({"type": "unsubscribe", "data": {"pattern": "audio.**"}}),
        );
    }

    #[test]
    fn request_call() {
        roundtrip_request(
            &Request::Call {
                method: "audio.set_volume".into(),
                params: json!({"node_id": 48, "volume": 0.5}),
            },
            json!({"type": "call", "data": {"method": "audio.set_volume", "params": {"node_id": 48, "volume": 0.5}}}),
        );
    }

    #[test]
    fn response_subscribe_ack() {
        roundtrip_response(
            &Response::SubscribeAck {
                pattern: "battery.**".into(),
                available: true,
                error: None,
            },
            json!({"type": "subscribe_ack", "data": {"pattern": "battery.**", "available": true}}),
        );
    }

    #[test]
    fn response_subscribe_ack_with_error() {
        roundtrip_response(
            &Response::SubscribeAck {
                pattern: "bluetooth.**".into(),
                available: false,
                error: Some("BlueZ not running".into()),
            },
            json!({"type": "subscribe_ack", "data": {"pattern": "bluetooth.**", "available": false, "error": "BlueZ not running"}}),
        );
    }

    #[test]
    fn response_unsubscribe_ack() {
        roundtrip_response(
            &Response::UnsubscribeAck {
                pattern: "audio.**".into(),
            },
            json!({"type": "unsubscribe_ack", "data": {"pattern": "audio.**"}}),
        );
    }

    #[test]
    fn response_event() {
        roundtrip_response(
            &Response::Event {
                topic: "battery.status".into(),
                data: json!({"percentage": 85, "state": "discharging"}),
            },
            json!({"type": "event", "data": {"topic": "battery.status", "data": {"percentage": 85, "state": "discharging"}}}),
        );
    }

    #[test]
    fn response_provider_unavailable() {
        roundtrip_response(
            &Response::ProviderUnavailable {
                provider: "bluetooth".into(),
                error: "BlueZ not running".into(),
            },
            json!({"type": "provider_unavailable", "data": {"provider": "bluetooth", "error": "BlueZ not running"}}),
        );
    }

    #[test]
    fn response_get_result_ok() {
        roundtrip_response(
            &Response::GetResult {
                topic: "battery.status".into(),
                result: RequestResult::Ok {
                    data: json!({"percentage": 85}),
                },
            },
            json!({"type": "get_result", "data": {"topic": "battery.status", "status": "ok", "data": {"percentage": 85}}}),
        );
    }

    #[test]
    fn response_get_result_error() {
        roundtrip_response(
            &Response::GetResult {
                topic: "bluetooth.devices".into(),
                result: RequestResult::Error {
                    code: 1,
                    message: "provider unavailable".into(),
                },
            },
            json!({"type": "get_result", "data": {"topic": "bluetooth.devices", "status": "error", "code": 1, "message": "provider unavailable"}}),
        );
    }

    #[test]
    fn response_call_result_ok() {
        roundtrip_response(
            &Response::CallResult {
                method: "audio.set_volume".into(),
                result: RequestResult::Ok { data: json!(null) },
            },
            json!({"type": "call_result", "data": {"method": "audio.set_volume", "status": "ok", "data": null}}),
        );
    }

    #[test]
    fn response_call_result_error() {
        roundtrip_response(
            &Response::CallResult {
                method: "audio.set_volume".into(),
                result: RequestResult::Error {
                    code: 4,
                    message: "invalid parameters".into(),
                },
            },
            json!({"type": "call_result", "data": {"method": "audio.set_volume", "status": "error", "code": 4, "message": "invalid parameters"}}),
        );
    }

    #[test]
    fn ndjson_line_has_no_raw_newlines() {
        let event = Response::Event {
            topic: "test".into(),
            data: json!({"text": "line1\nline2"}),
        };
        let line = serde_json::to_string(&event).unwrap();
        assert!(!line.contains('\n'), "serialized JSON must not contain raw newlines");
    }
}
