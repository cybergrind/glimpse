# glimpsed Daemon Design Plan

## Overview

A background daemon (`glimpsed`) that provides system services over a Unix socket using JSON for the protocol. Features:
- **Event streaming**: Clients subscribe to topics, receive real-time updates
- **Lazy connections**: Only connect to data sources (D-Bus, etc.) when clients are subscribed
- **Deduplication**: Multiple clients subscribing to the same topic share one upstream connection
- **Method calls**: Request-response pattern for one-off actions
- **One-shot reads**: Get current state without subscribing

## Decisions

- **Protocol**: NDJSON (newline-delimited JSON) over Unix socket
- **Socket**: `$XDG_RUNTIME_DIR/glimpsed.sock`
- **Serialization**: serde with adjacently tagged enums
- **Client library**: Yes, `glimpse-client` crate
- **Provider dispatch**: Trait objects (`Box<dyn Provider>`)
- **Error handling**: Acknowledge subscription with error status, then auto-send updates when source becomes available
- **Topic granularity**: Fine-grained with wildcard support (e.g., `bluetooth.devices`, `bluetooth.*`)
- **Auto-cleanup**: All subscriptions removed on client disconnect

---

## Protocol

### Framing

Newline-delimited JSON. One JSON object per line, `\n` as delimiter.

Debuggable with: `echo '{"type":"get","data":{"topic":"battery.status"}}' | socat - UNIX:$XDG_RUNTIME_DIR/glimpsed.sock`

### Messages

```rust
/// Client → Daemon
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum Request {
    Get { topic: String },
    Subscribe { pattern: String },
    Unsubscribe { pattern: String },
    Call { method: String, params: serde_json::Value },
}

/// Daemon → Client
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
enum Response {
    GetResult { topic: String, #[serde(flatten)] result: RequestResult },
    SubscribeAck { pattern: String, available: bool, error: Option<String> },
    UnsubscribeAck { pattern: String },
    CallResult { method: String, #[serde(flatten)] result: RequestResult },
    Event { topic: String, data: serde_json::Value },
    ProviderUnavailable { provider: String, error: String },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum RequestResult {
    Ok { data: serde_json::Value },
    Error { code: u32, message: String },
}
```

### Wire examples

```json
{"type":"subscribe","data":{"pattern":"battery.**"}}
{"type":"subscribe_ack","data":{"pattern":"battery.**","available":true}}
{"type":"event","data":{"topic":"battery.status","data":{"percentage":85,"state":"discharging"}}}
{"type":"get","data":{"topic":"audio.default_output"}}
{"type":"get_result","data":{"topic":"audio.default_output","status":"ok","data":{"id":48,"volume":0.75}}}
{"type":"call","data":{"method":"audio.set_volume","params":{"node_id":48,"volume":0.5}}}
{"type":"call_result","data":{"method":"audio.set_volume","status":"ok","data":null}}
```

### Wildcard patterns

- `*` matches a single segment: `bluetooth.*` matches `bluetooth.devices` but not `bluetooth.device.AA:BB:CC`
- `**` matches any number of segments: `bluetooth.**` matches everything under `bluetooth`

---

## Architecture

### Crate Structure

```
glimpse/
├── glimpsed/              # Daemon binary
│   ├── src/
│   │   ├── main.rs        # Entry point, signal handling, socket bind
│   │   ├── server.rs      # Unix socket accept loop, per-client tasks
│   │   ├── broker.rs      # Subscription management, event routing
│   │   ├── pattern.rs     # Wildcard pattern matching
│   │   └── providers/     # Data source implementations
│   │       ├── mod.rs     # Provider trait, factory, registry
│   │       ├── dbus_props.rs # DbusPropertyGroup helper
│   │       ├── debug.rs   # Test provider
│   │       └── ...        # 32 provider modules
│   └── Cargo.toml
├── glimpse-types/         # Shared JSON message types
│   ├── src/lib.rs         # Request, Response, provider-specific types
│   └── Cargo.toml
└── glimpse-client/        # Async Rust client library
    ├── src/lib.rs         # connect, subscribe->Stream, get, call
    └── Cargo.toml
```

### Provider Trait

```rust
trait Provider: Send + 'static {
    fn name(&self) -> &'static str;
    fn topics(&self) -> &'static [&'static str];
    fn methods(&self) -> &'static [&'static str];
    async fn run(&mut self, tx: mpsc::Sender<ProviderEvent>, cancel: CancellationToken) -> Result<()>;
    async fn snapshot(&self, topic: &str) -> Option<serde_json::Value>;
    async fn handle_call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value>;
}

struct ProviderEvent {
    topic: String,
    data: serde_json::Value,
}
```

### Provider Factory

```rust
trait ProviderFactory: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn topics(&self) -> &'static [&'static str];
    fn methods(&self) -> &'static [&'static str];
    fn create(&self) -> Box<dyn Provider>;
}
```

### Broker

Single task, no locks — all mutations via mpsc channel:

```rust
enum BrokerMsg {
    ClientConnected { id: ClientId, tx: mpsc::Sender<Response> },
    ClientDisconnected { id: ClientId },
    Request { client: ClientId, request: Request },
    ProviderEvent { topic: String, data: serde_json::Value },
    ProviderStopped { name: &'static str, error: Option<String> },
}
```

Lazy lifecycle: start provider on first matching subscribe, stop when last subscriber leaves (with grace period).

### Server

Per-client: reader task (BufReader::read_line → parse JSON → broker channel) + writer task (serialize JSON → write line → flush).

Auto-cleanup: when socket EOF/error detected, send `ClientDisconnected` to broker → all subscriptions removed.

### Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "net", "sync", "io-util", "signal"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
zbus = "5"
tracing = "0.1"
tracing-subscriber = "0.3"
tokio-util = "0.7"  # CancellationToken
anyhow = "1"
```

---

## Implementation Phases

### Phase 1: Types crate (`glimpse-types`)
1. Create crate with Request, Response, RequestResult types
2. Shared between daemon and client

### Phase 2: Daemon crate (`glimpsed`)
3. main.rs — entry point, signal handling, socket bind
4. server.rs — accept loop, per-client reader/writer tasks
5. broker.rs — subscription management, event routing, provider lifecycle
6. pattern.rs — wildcard pattern matching
7. providers/mod.rs — Provider trait, ProviderFactory, registry
8. providers/debug.rs — test provider emitting periodic events

### Phase 3: Client library (`glimpse-client`)
9. connect, subscribe → Stream, get, call, auto-reconnect

### Phase 4: Real providers
10. Implement providers one at a time (battery, power, audio, ...)

---

## Verification

1. `cargo build --workspace`
2. Start daemon: `cargo run -p glimpsed`
3. Test with socat:
   - `echo '{"type":"get","data":{"topic":"debug.counter"}}' | socat - UNIX:$XDG_RUNTIME_DIR/glimpsed.sock`
   - `echo '{"type":"subscribe","data":{"pattern":"debug.*"}}' | socat - UNIX:$XDG_RUNTIME_DIR/glimpsed.sock` (prints ack + events)
4. Multiple clients: two socat sessions, both subscribe, both receive events
