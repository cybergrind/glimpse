# glimpsed — Architecture

## Daemon Components

```
main.rs          — entry point, signal handling, socket bind
server.rs        — accept loop, per-client reader/writer tasks
broker.rs        — subscription management, event routing, provider lifecycle
pattern.rs       — wildcard pattern matching (*, **)
framing.rs       — length-prefixed protobuf read/write
providers/
  mod.rs         — Provider trait, ProviderFactory, registry
  dbus_props.rs  — DbusPropertyGroup helper for D-Bus providers
  compositor/
    mod.rs       — CompositorBackend trait + autodetect
    niri.rs      — niri IPC socket
    hyprland.rs  — hyprland IPC socket
  ...            — one file per provider
```

## Startup Flow

1. Parse CLI args (socket path, config path, log level)
2. Initialize tracing
3. Load config from `$XDG_CONFIG_HOME/glimpse/glimpsed.toml`
4. Detect compositor (check `$NIRI_SOCKET`, `$HYPRLAND_INSTANCE_SIGNATURE`)
5. Connect to Wayland display (for gamma control, idle notify, clipboard, data control)
6. Connect to session D-Bus and system D-Bus
7. Remove stale socket, bind `UnixListener` at `$XDG_RUNTIME_DIR/glimpsed.sock`
8. Create Broker with provider factories
9. Spawn server accept loop
10. Wait for SIGTERM/SIGINT via `CancellationToken`

## Server

Per-client architecture:
- Accept connection → assign `ClientId` (monotonic u64)
- Spawn **reader task**: read length-prefixed frames → deserialize `Request` → send to broker channel
- Spawn **writer task**: receive `Response` from `mpsc::Receiver` → serialize → write frames

## Broker

Single task processing `BrokerMessage` from an mpsc channel — no locks.

Messages:
- `ClientConnected { id, tx }` / `ClientDisconnected { id }`
- `Subscribe { client, request_id, pattern }` / `Unsubscribe { client, request_id, pattern }`
- `Get { client, request_id, topic }`
- `MethodCall { client, request_id, method, params }`
- `ProviderEvent { topic, data }`

Lifecycle:
- **Lazy start**: first subscribe matching a provider → start provider
- **Snapshot on subscribe**: provider sends current state immediately
- **Grace period**: last subscriber leaves → wait 5s → stop provider (avoids thrashing)
- **Crash recovery**: provider crashes → log, notify subscribers with `ProviderUnavailable`, restart with backoff

## Provider Trait

```rust
trait Provider: Send + 'static {
    fn name(&self) -> &'static str;
    fn topics(&self) -> &'static [&'static str];
    async fn run(&mut self, events_tx: mpsc::Sender<ProviderEvent>, cancel: CancellationToken) -> Result<()>;
    async fn handle_call(&self, method: &str, params: &[u8]) -> Result<Vec<u8>>;
    async fn snapshot(&self, topic: &str) -> Option<Vec<u8>>;
}
```

Each provider:
- Runs in its own tokio task
- Sends events through `events_tx` channel
- Can be cancelled via `CancellationToken`
- Handles method calls concurrently with `run()`
- Returns current state via `snapshot()` for new subscribers and `Get` requests

## DbusPropertyGroup Helper

Reduces boilerplate for D-Bus providers:

```rust
struct DbusPropertyGroup { ... }

impl DbusPropertyGroup {
    async fn get<T>(&self, name: &str) -> Option<T>;
    async fn set<T>(&self, name: &str, value: T) -> Result<()>;
    async fn call<A, R>(&self, method: &str, args: &A) -> Result<R>;
    async fn stream_changes(&self) -> Result<impl Stream<Item = Vec<String>>>;
}
```

Used by: battery, power, bluetooth, wifi, network, mpris, theme, geolocation, removable_media, sessions, locale.

## Compositor Backend

```rust
trait CompositorBackend: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    async fn list_workspaces(&self) -> Result<Vec<Workspace>>;
    async fn switch_workspace(&self, reference: &WorkspaceReference) -> Result<()>;
    async fn subscribe_workspaces(&self, tx: mpsc::Sender<...>, cancel: CancellationToken) -> Result<()>;
    async fn current_layout(&self) -> Result<KeyboardLayout>;
    async fn set_layout(&self, index: u32) -> Result<()>;
    async fn subscribe_layout(&self, tx: mpsc::Sender<...>, cancel: CancellationToken) -> Result<()>;
}
```

Detection: `$NIRI_SOCKET` → niri, `$HYPRLAND_INSTANCE_SIGNATURE` → hyprland, else unavailable.

Used by: workspaces, keyboard providers.

## Wayland Connection

The daemon also acts as a Wayland client for protocols that require it:
- `wlr-gamma-control-unstable-v1` — nightlight provider
- `ext-idle-notify-v1` — idle provider
- `ext-data-control-v1` / `wlr-data-control-unstable-v1` — clipboard provider

This is separate from the Unix socket IPC — the daemon is both a Wayland client and a socket server.

## Pattern Matching

Topics are dot-separated: `battery.status`, `bluetooth.device.AA:BB:CC:DD:EE:FF`

- `*` matches exactly one segment: `bluetooth.*` matches `bluetooth.devices` but not `bluetooth.device.AA:BB`
- `**` matches zero or more segments: `bluetooth.**` matches everything under bluetooth
- Literal segments match exactly

## Event Flow

```
System service (D-Bus, /proc, Wayland, HTTP)
    ↓
Provider (monitors changes, parses data)
    ↓
ProviderEvent { topic, data }
    ↓
Broker (matches against subscription patterns)
    ↓
Response { Event { topic, data } }
    ↓
Client writer task (serialize, write to socket)
    ↓
Client (panel, CLI, widget)
```
