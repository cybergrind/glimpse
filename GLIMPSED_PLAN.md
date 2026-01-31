# glimpsed Daemon Design Plan

## Overview

A background daemon (`glimpsed`) that provides system services over a Unix socket using protobuf for the protocol. Features:
- **Event streaming**: Clients subscribe to topics, receive real-time updates
- **Lazy connections**: Only connect to data sources (D-Bus, etc.) when clients are subscribed
- **Deduplication**: Multiple clients subscribing to the same topic share one upstream connection
- **Method calls**: Request-response pattern for one-off data retrieval

## Decisions

- **Socket**: `$XDG_RUNTIME_DIR/glimpsed.sock`
- **Client library**: Yes, `glimpse-client` crate
- **Error handling**: Acknowledge subscription with error status, then auto-send updates when source becomes available
- **Topic granularity**: Fine-grained with wildcard support (e.g., `bluetooth.devices`, `bluetooth.*`)

---

## Proposed Architecture

### Crate Structure

```
glimpse/
├── glimpsed/              # Daemon binary
│   ├── src/
│   │   ├── main.rs        # Entry point, socket server
│   │   ├── server.rs      # Unix socket server, client handling
│   │   ├── broker.rs      # Subscription management, deduplication
│   │   └── providers/     # Data source implementations
│   │       ├── mod.rs
│   │       ├── bluetooth.rs
│   │       └── network.rs
│   └── Cargo.toml
├── glimpse-proto/         # Shared protobuf definitions
│   ├── proto/
│   │   └── glimpse.proto
│   ├── src/lib.rs
│   └── Cargo.toml
└── glimpse-client/        # Rust client library
```

### Core Components

#### 1. Protocol (protobuf)

```protobuf
// glimpse.proto

// Client -> Daemon
message Request {
  uint64 id = 1;
  oneof payload {
    Subscribe subscribe = 2;
    Unsubscribe unsubscribe = 3;
    MethodCall method_call = 4;
  }
}

message Subscribe {
  string pattern = 1;  // e.g., "bluetooth.devices", "bluetooth.*", "network.**"
}

message Unsubscribe {
  string pattern = 1;
}

message MethodCall {
  string method = 1;  // e.g., "bluetooth.connect"
  bytes params = 2;   // Method-specific protobuf message
}

// Daemon -> Client
message Response {
  uint64 request_id = 1;
  oneof payload {
    SubscribeAck subscribe_ack = 2;
    UnsubscribeAck unsubscribe_ack = 3;
    MethodResult method_result = 4;
    Event event = 5;
  }
}

message SubscribeAck {
  bool available = 1;       // Is the provider currently available?
  string error_message = 2; // If not available, why (empty if available)
}

message UnsubscribeAck {}

message MethodResult {
  oneof result {
    bytes data = 1;
    Error error = 2;
  }
}

message Event {
  string topic = 1;   // Actual topic that fired (not the pattern)
  bytes data = 2;     // Topic-specific protobuf message
}

message Error {
  uint32 code = 1;
  string message = 2;
}
```

**Wildcard patterns:**
- `*` matches a single segment: `bluetooth.*` matches `bluetooth.devices` but not `bluetooth.device.AA:BB:CC`
- `**` matches any number of segments: `bluetooth.**` matches everything under `bluetooth`

#### 2. Subscription Broker

The broker manages:
- Client subscriptions (client_id -> set of patterns)
- Pattern matching for event routing
- Provider lifecycle (start/stop based on subscriber count)

```rust
struct Broker {
    // Client's active subscription patterns
    subscriptions: HashMap<ClientId, HashSet<Pattern>>,
    // Which providers are currently running
    active_providers: HashMap<ProviderName, ProviderHandle>,
    // Provider registry
    providers: HashMap<ProviderName, Box<dyn ProviderFactory>>,
}

impl Broker {
    fn subscribe(&mut self, client: ClientId, pattern: Pattern) -> SubscribeAck {
        self.subscriptions.entry(client).or_default().insert(pattern.clone());
        
        // Determine which provider(s) this pattern requires
        let provider_name = pattern.provider_name(); // e.g., "bluetooth" from "bluetooth.*"
        
        // Start provider if not running
        if !self.active_providers.contains_key(&provider_name) {
            match self.start_provider(&provider_name) {
                Ok(handle) => {
                    self.active_providers.insert(provider_name, handle);
                    SubscribeAck { available: true, error_message: String::new() }
                }
                Err(e) => {
                    // Provider unavailable, but subscription is registered
                    // Will receive events when provider becomes available
                    SubscribeAck { available: false, error_message: e.to_string() }
                }
            }
        } else {
            SubscribeAck { available: true, error_message: String::new() }
        }
    }
    
    fn route_event(&self, topic: &str, data: &[u8]) -> Vec<ClientId> {
        // Find all clients whose patterns match this topic
        self.subscriptions.iter()
            .filter(|(_, patterns)| patterns.iter().any(|p| p.matches(topic)))
            .map(|(client, _)| *client)
            .collect()
    }
    
    fn unsubscribe(&mut self, client: ClientId, pattern: &Pattern) {
        if let Some(patterns) = self.subscriptions.get_mut(&client) {
            patterns.remove(pattern);
        }
        self.maybe_stop_unused_providers();
    }
    
    fn client_disconnected(&mut self, client: ClientId) {
        self.subscriptions.remove(&client);
        self.maybe_stop_unused_providers();
    }
    
    fn maybe_stop_unused_providers(&mut self) {
        // Stop providers that no longer have any matching subscriptions
    }
}
```

#### 3. Provider Trait

```rust
#[async_trait]
trait Provider: Send + Sync {
    /// Provider name (e.g., "bluetooth", "network")
    fn name(&self) -> &'static str;
    
    /// List of topics this provider can emit
    fn topics(&self) -> &[&'static str];
    
    /// Start collecting data, send events through the channel
    async fn run(&mut self, events: mpsc::Sender<ProviderEvent>) -> Result<()>;
    
    /// Handle method calls (called concurrently with run)
    async fn call(&self, method: &str, params: &[u8]) -> Result<Vec<u8>>;
}

struct ProviderEvent {
    topic: String,      // e.g., "bluetooth.device.AA:BB:CC"
    data: Vec<u8>,      // Protobuf-encoded payload
}
```

#### 4. Socket Server

- Accept connections on Unix socket
- Length-prefixed protobuf messages (4-byte big-endian length + message)
- Each client gets a task for reading and a channel for writing
- Graceful shutdown handling

### Message Framing

```
+----------------+------------------+
| Length (4 bytes, big-endian)     |
+----------------+------------------+
| Protobuf Message (Length bytes)  |
+----------------------------------+
```

### Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "net", "sync", "io-util", "signal"] }
prost = "0.13"           # Protobuf runtime
prost-types = "0.13"     # Well-known types
zbus = "5"               # D-Bus for system services
tracing = "0.1"
tracing-subscriber = "0.3"

[build-dependencies]
prost-build = "0.13"     # Protobuf code generation
```

---

## Implementation Plan

### Phase 1: Proto crate (`glimpse-proto`)
1. Create crate with `proto/glimpse.proto`
2. Set up `prost-build` in `build.rs`
3. Export generated types from `lib.rs`

### Phase 2: Daemon crate (`glimpsed`)
4. Create crate structure:
   - `main.rs` - entry point, signal handling
   - `server.rs` - Unix socket accept loop, client tasks
   - `broker.rs` - subscription management, event routing
   - `pattern.rs` - wildcard pattern matching
   - `providers/mod.rs` - provider trait and registry
5. Implement message framing (length-prefixed protobuf)
6. Implement pattern matching (`*` and `**` wildcards)
7. Implement broker with lazy provider lifecycle
8. Add a stub provider for testing (e.g., `debug` provider that emits periodic events)

### Phase 3: Client library (`glimpse-client`)
9. Create crate with async client API:
   - `Client::connect()` 
   - `client.subscribe(pattern)` -> `Stream<Event>`
   - `client.call(method, params)` -> `Result<Response>`
   - Auto-reconnect handling

### Phase 4: Additional Providers (future)
- Implement real providers as needed (bluetooth, network, etc.)

---

## Files to Create

```
glimpse/
├── Cargo.toml                    # Add workspace members
├── glimpse-proto/
│   ├── Cargo.toml
│   ├── build.rs
│   ├── proto/
│   │   └── glimpse.proto
│   └── src/
│       └── lib.rs
├── glimpsed/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── server.rs
│       ├── broker.rs
│       ├── pattern.rs
│       └── providers/
│           ├── mod.rs
│           └── debug.rs
└── glimpse-client/
    ├── Cargo.toml
    └── src/
        └── lib.rs
```

---

## Verification

1. `cargo build --workspace`
2. Start daemon: `cargo run -p glimpsed`
3. Write a simple test binary in `glimpse-client/examples/` that:
   - Connects to the socket
   - Subscribes to `debug.*`
   - Prints received events
   - Unsubscribes and disconnects
