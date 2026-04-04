# glimpsed — Development Tasks

## Phase 1: Foundation

### 1.1 Create `glimpse-types` crate
- [ ] Create `glimpse-types/Cargo.toml` (serde, serde_json)
- [ ] Define `Request` enum (Get, Subscribe, Unsubscribe, Call) — see [02-protocol.md](02-protocol.md)
- [ ] Define `Response` enum (GetResult, SubscribeAck, UnsubscribeAck, CallResult, Event, ProviderUnavailable)
- [ ] Define `RequestResult` enum (Ok, Error)
- [ ] Unit tests: serialize/deserialize each variant, verify JSON format matches wire examples
- [ ] Add to workspace `Cargo.toml`

### 1.2 Create `glimpsed` crate skeleton
- [ ] Create `glimpsed/Cargo.toml`
- [ ] `main.rs` — CLI args (socket path, config path, log level), tracing init, socket bind, signal handling (SIGTERM/SIGINT via CancellationToken)
- [ ] Remove stale socket file on startup
- [ ] Add to workspace `Cargo.toml`

### 1.3 Implement pattern matching
- [ ] `pattern.rs` — `Pattern` struct, parse from string, `matches(topic: &str) -> bool`
- [ ] Support `*` (one segment) and `**` (any depth) wildcards
- [ ] Unit tests: exact match, single wildcard, deep wildcard, no match, edge cases (empty, root)

### 1.4 Implement server
- [ ] `server.rs` — `UnixListener` accept loop
- [ ] Per-client: spawn reader task (BufReader::read_line → parse JSON) + writer task (serialize → write line)
- [ ] Assign monotonic `ClientId` per connection
- [ ] Detect client disconnect (EOF/error) → send `ClientDisconnected` to broker
- [ ] Integration test: connect, send request, receive response

### 1.5 Implement broker
- [ ] `broker.rs` — single task consuming `BrokerMsg` from mpsc channel
- [ ] Handle `ClientConnected`, `ClientDisconnected`, `Request`, `ProviderEvent`, `ProviderStopped`
- [ ] Subscription management: add/remove patterns per client
- [ ] Event routing: match ProviderEvent topic against all client subscriptions
- [ ] Provider lifecycle: lazy start on first subscribe, stop on last unsubscribe (with grace period)
- [ ] Get handling: snapshot from provider, respond to client
- [ ] Call handling: route to provider's handle_call, respond to client
- [ ] Auto-cleanup on disconnect: remove all subscriptions, stop unused providers
- [ ] Unit tests: subscribe/unsubscribe, event routing, disconnect cleanup

### 1.6 Implement Provider trait and registry
- [ ] `providers/mod.rs` — `Provider` trait, `ProviderFactory` trait, `ProviderEvent` struct
- [ ] `ProviderHandle` — wraps JoinHandle + CancellationToken
- [ ] Provider registration: `Vec<Box<dyn ProviderFactory>>` → topic/method routing tables

### 1.7 Debug provider
- [ ] `providers/debug.rs` — emits `debug.counter` (incrementing number) and `debug.timestamp` (current time) every second
- [ ] Responds to `debug.echo` method call (returns params back)
- [ ] Used for end-to-end testing

### 1.8 End-to-end verification
- [ ] Start daemon, connect with `socat`, send subscribe, receive events
- [ ] Test Get, Subscribe, Unsubscribe, Call with debug provider
- [ ] Test multiple simultaneous clients
- [ ] Test client disconnect cleanup
- [ ] Test provider lazy start/stop

---

## Phase 2: Client library

### 2.1 Create `glimpse-client` crate
- [ ] Create `glimpse-client/Cargo.toml`
- [ ] `connect()` — connect to default socket path (`$XDG_RUNTIME_DIR/glimpsed.sock`)
- [ ] `connect_to(path)` — connect to specific path
- [ ] Internal reader/writer tasks with mpsc channels

### 2.2 Client API
- [ ] `get(topic) -> Result<Value>` — one-shot read
- [ ] `subscribe(pattern) -> Result<Subscription>` — returns stream of events
- [ ] `unsubscribe(pattern) -> Result<()>`
- [ ] `call(method, params) -> Result<Value>` — method call
- [ ] `Subscription` implements `Stream<Item = Event>`
- [ ] Request ID tracking: match responses to requests

### 2.3 Auto-reconnect
- [ ] Detect socket disconnect
- [ ] Reconnect with exponential backoff
- [ ] Re-send all active subscriptions after reconnect
- [ ] Subscription streams pause during reconnect, resume after

### 2.4 Example binary
- [ ] `glimpse-client/examples/subscribe.rs` — connect, subscribe to `debug.*`, print events
- [ ] Verify roundtrip works end-to-end

---

## Phase 3: TUI tool

### 3.1 Create `glimpsectl` crate
- [ ] Create `glimpsectl/Cargo.toml` (ratatui, crossterm, tui-textarea, nucleo-matcher, glimpse-types)
- [ ] `main.rs` — terminal setup (raw mode, alternate screen), event loop, cleanup

### 3.2 App state
- [ ] `app.rs` — App struct with message log, input state, picker state, connection state
- [ ] Message struct with direction (in/out), timestamp, raw JSON, parsed value

### 3.3 UI rendering
- [ ] `ui.rs` — two-pane vertical split layout
- [ ] Top pane: scrollable message log with colored JSON, direction markers (→/←), timestamps
- [ ] Bottom pane: text input + `[Commands]` button indicator
- [ ] Connection status in title bar

### 3.4 Input handling
- [ ] `input.rs` — parse short command syntax: `get topic`, `sub pattern`, `unsub pattern`, `call method {params}`
- [ ] Also accept raw JSON input
- [ ] Command history (up/down arrows)

### 3.5 Command picker modal
- [ ] `picker.rs` — popup overlay with search input + scrollable results list
- [ ] `catalog.rs` — all known topics and methods from all 32 providers
- [ ] Fuzzy search with nucleo-matcher
- [ ] Enter selects → fills input, Esc cancels

### 3.6 Socket integration
- [ ] Connect to glimpsed socket on startup
- [ ] Send commands from input
- [ ] Receive responses/events → add to message log
- [ ] Auto-scroll on new messages (unless user scrolled up)
- [ ] Reconnect on disconnect

---

## Phase 4: First real providers

### 4.1 DbusPropertyGroup helper
- [ ] `providers/dbus_props.rs` — see [01-architecture.md](01-architecture.md)
- [ ] `get<T>(name)`, `set<T>(name, value)`, `call<A,R>(method, args)`, `stream_changes()`
- [ ] Shared by: battery, power, bluetooth, wifi, network, mpris, theme, geolocation, removable_media, sessions, locale

### 4.2 Battery provider
- [ ] `providers/battery.rs` — see [battery.md](battery.md)
- [ ] Topics: `battery.status`, `battery.devices`
- [ ] D-Bus: UPower (system bus)
- [ ] Change detection: PropertiesChanged, DeviceAdded/Removed
- [ ] Verify: `get battery.status` returns real data, subscribe receives live updates

### 4.3 Power provider
- [ ] `providers/power.rs` — see [power.md](power.md)
- [ ] Topics: `power.profiles`, `power.actions`, `power.inhibitors`
- [ ] Methods: `power.set_profile`, `power.suspend`, `power.hibernate`, `power.reboot`, `power.poweroff`, `power.lock`, `power.inhibit`, `power.release_inhibit`, etc.
- [ ] D-Bus: logind + PowerProfiles (system bus)
- [ ] Verify: get profiles, call suspend

### 4.4 Compositor backend
- [ ] `providers/compositor/mod.rs` — `CompositorBackend` trait, auto-detect from env vars
- [ ] `providers/compositor/niri.rs` — niri IPC via niri-ipc crate
- [ ] `providers/compositor/hyprland.rs` — hyprland IPC via hyprland crate
- [ ] Verify: detect compositor, query workspaces

---

## Phase 5: Remaining providers (priority order)

Each provider task includes: implement provider, add to registry, test with glimpsectl.

### Tier 1 — Core panel features
- [ ] tray — see [tray.md](tray.md) — StatusNotifierItem/Watcher, DBusMenu
- [ ] audio — see [audio.md](audio.md) — PulseAudio D-Bus or PipeWire
- [ ] mpris — see [mpris.md](mpris.md) — MPRIS2 D-Bus
- [ ] network — see [network.md](network.md) — NetworkManager (ethernet, VPN)
- [ ] bluetooth — see [bluetooth.md](bluetooth.md) — BlueZ via bluer crate
- [ ] wifi — see [wifi.md](wifi.md) — NetworkManager (wireless)

### Tier 2 — Desktop integration
- [ ] apps — see [apps.md](apps.md) — .desktop file indexing
- [ ] workspaces — see [workspaces.md](workspaces.md) — compositor IPC
- [ ] keyboard — see [keyboard.md](keyboard.md) — compositor IPC
- [ ] brightness — see [brightness.md](brightness.md) — sysfs + ddcutil + iio-sensor
- [ ] system_stats — see [system_stats.md](system_stats.md) — /proc, /sys
- [ ] theme — see [theme.md](theme.md) — XDG portal + gsettings
- [ ] notifications — see [notifications.md](notifications.md) — freedesktop Notifications server

### Tier 3 — Extended features
- [ ] clock — see [clock.md](clock.md) — system clock + chrono
- [ ] privacy — see [privacy.md](privacy.md) — meta-provider aggregating audio/camera/screen
- [ ] idle — see [idle.md](idle.md) — ext-idle-notify + logind
- [ ] sessions — see [sessions.md](sessions.md) — logind sessions
- [ ] clipboard — see [clipboard.md](clipboard.md) — Wayland data control
- [ ] nightlight — see [nightlight.md](nightlight.md) — wlr-gamma-control + GeoClue2
- [ ] geolocation — see [geolocation.md](geolocation.md) — GeoClue2
- [ ] removable_media — see [removable_media.md](removable_media.md) — udisks2

### Tier 4 — Nice to have
- [ ] locale — see [locale.md](locale.md) — timedate1
- [ ] weather — see [weather.md](weather.md) — Open-Meteo HTTP
- [ ] display — see [display.md](display.md) — compositor IPC + DRM
- [ ] airplane — see [airplane.md](airplane.md) — rfkill
- [ ] screen_capture — see [screen_capture.md](screen_capture.md) — XDG portal
- [ ] camera — see [camera.md](camera.md) — PipeWire
- [ ] accessibility — see [accessibility.md](accessibility.md) — gsettings
- [ ] printers — see [printers.md](printers.md) — CUPS
- [ ] calendar — see [calendar.md](calendar.md) — CalDAV + GOA

---

## Phase 6: Panel migration

- [ ] Add `glimpse-client` as dependency of `glimpse-panel`
- [ ] Replace power applet's direct D-Bus with client subscriptions
- [ ] Replace tray applet's system-tray crate with client subscriptions
- [ ] Remove `zbus` as direct panel dependency
- [ ] Remove panel's DbusProvider
- [ ] Verify: panel works with daemon for all applets
