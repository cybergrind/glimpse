# glimpsed — Communication Protocol

## Transport

- Unix domain socket at `$XDG_RUNTIME_DIR/glimpsed.sock`
- NDJSON framing: one JSON object per line, `\n` as delimiter

## Message Types

### Client → Daemon (Request)

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum Request {
    /// One-shot read of current state
    Get { topic: String },

    /// Subscribe to live updates (supports wildcards)
    Subscribe { pattern: String },

    /// Remove a subscription
    Unsubscribe { pattern: String },

    /// One-shot action (e.g. set volume, connect wifi)
    Call { method: String, params: serde_json::Value },
}
```

### Daemon → Client (Response)

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum Response {
    /// Reply to Get
    GetResult {
        topic: String,
        #[serde(flatten)]
        result: RequestResult,
    },

    /// Reply to Subscribe
    SubscribeAck {
        pattern: String,
        available: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Reply to Unsubscribe
    UnsubscribeAck { pattern: String },

    /// Reply to Call
    CallResult {
        method: String,
        #[serde(flatten)]
        result: RequestResult,
    },

    /// Live event from subscription
    Event {
        topic: String,
        data: serde_json::Value,
    },

    /// Provider became unavailable
    ProviderUnavailable {
        provider: String,
        error: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum RequestResult {
    Ok { data: serde_json::Value },
    Error { code: u32, message: String },
}
```

## Request/Response Semantics

### Get

One-shot read. Client sends `Get { topic }`, daemon responds with `GetResult` containing the current snapshot. No subscription created. If the provider is not running, it starts temporarily to serve the request.

### Subscribe

Live stream. Client sends `Subscribe { pattern }`, daemon responds with:
1. `SubscribeAck` — whether the provider is available
2. Immediate `Event` with current state (snapshot)
3. Ongoing `Event` messages whenever the topic data changes

Pattern matching:
- `battery.status` — exact topic match
- `bluetooth.*` — all topics one level under bluetooth
- `audio.**` — all topics at any depth under audio

### Unsubscribe

Client sends `Unsubscribe { pattern }`. Daemon responds with `UnsubscribeAck`. If this was the last subscriber for a provider, the provider stops after a grace period.

### Call

One-shot action. Client sends `Call { method, params }`, daemon responds with `CallResult`. Methods are `{provider}.{action}` format. Params is provider-specific JSON.

### Auto-disconnect cleanup

When a client's socket closes (EOF or error), the daemon automatically removes all subscriptions for that client. If this was the last subscriber for a provider, the provider stops after a grace period. No explicit unsubscribe needed on disconnect.

## Wire Examples

Subscribe to battery updates:
```
→ {"type":"subscribe","data":{"pattern":"battery.**"}}
← {"type":"subscribe_ack","data":{"pattern":"battery.**","available":true}}
← {"type":"event","data":{"topic":"battery.status","data":{"percentage":85,"state":"discharging","icon_name":"battery-level-80-symbolic"}}}
← {"type":"event","data":{"topic":"battery.devices","data":[{"id":"battery_BAT0","device_type":"battery","percentage":85.0}]}}
... (ongoing events as state changes)
```

One-shot read:
```
→ {"type":"get","data":{"topic":"audio.default_output"}}
← {"type":"get_result","data":{"topic":"audio.default_output","status":"ok","data":{"id":48,"name":"Built-in Audio","volume":0.75,"muted":false}}}
```

Method call:
```
→ {"type":"call","data":{"method":"audio.set_volume","params":{"node_id":48,"volume":0.5}}}
← {"type":"call_result","data":{"method":"audio.set_volume","status":"ok","data":null}}
```

Error response:
```
→ {"type":"get","data":{"topic":"bluetooth.devices"}}
← {"type":"get_result","data":{"topic":"bluetooth.devices","status":"error","code":1,"message":"provider unavailable: BlueZ not running"}}
```

## Error Codes

- `1` — provider unavailable
- `2` — unknown topic
- `3` — unknown method
- `4` — invalid parameters
- `5` — method failed
- `6` — permission denied

## Event Data

Event payloads are `serde_json::Value` — provider-specific types serialized to JSON. The broker and protocol layer never see provider-specific Rust types. Clients deserialize based on the topic they subscribed to.

Each provider defines its own serde types (e.g. `BatteryStatus`, `AudioDevice`) which are serialized to `Value` via `serde_json::to_value()` before entering the broker.

## Testing with socat

```bash
# One-shot get
echo '{"type":"get","data":{"topic":"battery.status"}}' | socat - UNIX:$XDG_RUNTIME_DIR/glimpsed.sock

# Subscribe (keeps connection open, prints events)
echo '{"type":"subscribe","data":{"pattern":"battery.**"}}' | socat - UNIX:$XDG_RUNTIME_DIR/glimpsed.sock

# Call a method
echo '{"type":"call","data":{"method":"power.suspend","params":{}}}' | socat - UNIX:$XDG_RUNTIME_DIR/glimpsed.sock

# Pretty-print with jq
echo '{"type":"get","data":{"topic":"wifi.stations"}}' | socat - UNIX:$XDG_RUNTIME_DIR/glimpsed.sock | jq .
```
