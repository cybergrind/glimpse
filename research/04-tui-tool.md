# glimpsectl — TUI Debug/Test Tool

A terminal UI tool for testing and debugging the glimpsed daemon interactively.

## Layout

```
┌─────────────────────────────────────────────────────────┐
│ Messages                                          [Ctrl+Q] │
│─────────────────────────────────────────────────────────│
│ → {"type":"subscribe","data":{"pattern":"battery.**"}}  │
│ ← {"type":"subscribe_ack","data":{"pattern":"battery.** │
│    ","available":true}}                                 │
│ ← {"type":"event","data":{"topic":"battery.status",     │
│    "data":{"percentage":85,"state":"discharging"}}}     │
│ → {"type":"get","data":{"topic":"audio.default_output"}}│
│ ← {"type":"get_result","data":{"topic":"audio.default_  │
│    output","status":"ok","data":{"id":48,"volume":0.75}}│
│                                                         │
│                                                         │
│                                                         │
│─────────────────────────────────────────────────────────│
│ > subscribe battery.**                        [Commands]│
└─────────────────────────────────────────────────────────┘
```

Top pane: scrollable message log with colored JSON. Outgoing (→) in cyan, incoming (←) in green. Errors in red.

Bottom pane: text input for typing commands. `[Commands]` button at the right opens the command picker modal.

### Command picker modal

```
┌───────────── Commands ─────────────┐
│ Search: bat█                       │
│────────────────────────────────────│
│ > get battery.status               │
│   get battery.devices              │
│   subscribe battery.**             │
│   subscribe battery.status         │
│────────────────────────────────────│
│ [Enter] Select  [Esc] Cancel      │
└────────────────────────────────────┘
```

Fuzzy search over all known topics and methods. Selecting fills the input. For methods with parameters, shows a parameter form.

## Input Syntax

Short commands that get translated to JSON requests:

```
get battery.status              → {"type":"get","data":{"topic":"battery.status"}}
sub battery.**                  → {"type":"subscribe","data":{"pattern":"battery.**"}}
unsub battery.**                → {"type":"unsubscribe","data":{"pattern":"battery.**"}}
call audio.set_volume {"node_id":48,"volume":0.5}
                                → {"type":"call","data":{"method":"audio.set_volume","params":{"node_id":48,"volume":0.5}}}
```

Also accepts raw JSON: `{"type":"get","data":{"topic":"battery.status"}}`

## Keybindings

- `Enter` — send command
- `Ctrl+P` or `Tab` — open command picker modal
- `Ctrl+Q` — quit
- `Up/Down` — scroll message log
- `PageUp/PageDown` — scroll message log fast
- `Ctrl+L` — clear message log
- `Ctrl+C` — cancel / close modal
- `Esc` — close modal

In command picker:
- Type to fuzzy search
- `Up/Down` — navigate results
- `Enter` — select command
- `Esc` — cancel

## Architecture

```rust
struct App {
    /// Message log (outgoing + incoming)
    messages: Vec<Message>,
    /// Scroll position in message log
    scroll: usize,
    /// Text input state
    input: tui_textarea::TextArea,
    /// Command picker state (None = closed)
    picker: Option<CommandPicker>,
    /// Socket connection
    socket: Option<BufStream<UnixStream>>,
    /// Connection status
    connected: bool,
}

struct Message {
    direction: Direction,
    timestamp: chrono::DateTime<chrono::Local>,
    raw: String,
    /// Parsed for display (colored JSON)
    parsed: Option<serde_json::Value>,
}

enum Direction {
    Outgoing,
    Incoming,
}

struct CommandPicker {
    query: String,
    results: Vec<CommandTemplate>,
    selected: usize,
}

struct CommandTemplate {
    /// Short command (e.g. "get battery.status")
    command: String,
    /// Description
    description: String,
    /// Category for grouping
    category: String,
}
```

### Event loop

```rust
loop {
    // Render
    terminal.draw(|f| ui::draw(f, &mut app))?;

    tokio::select! {
        // Terminal input events
        Some(event) = event_stream.next() => {
            handle_input(&mut app, event);
        }
        // Incoming messages from daemon
        Some(line) = socket_reader.next_line() => {
            app.messages.push(Message::incoming(line));
            app.scroll_to_bottom();
        }
    }
}
```

### Command catalog

Built from the research docs — all known topics, methods, and their parameter schemas:

```rust
fn build_catalog() -> Vec<CommandTemplate> {
    vec![
        // Gets
        cmd("get battery.status", "Current battery state", "battery"),
        cmd("get battery.devices", "All UPower devices", "battery"),
        cmd("get power.profiles", "Power profiles", "power"),
        cmd("get audio.default_output", "Default audio output", "audio"),
        // ...

        // Subscribes
        cmd("sub battery.**", "All battery events", "battery"),
        cmd("sub audio.**", "All audio events", "audio"),
        cmd("sub bluetooth.**", "All bluetooth events", "bluetooth"),
        // ...

        // Method calls
        cmd("call power.suspend {}", "Suspend system", "power"),
        cmd("call audio.set_volume {\"node_id\":0,\"volume\":0.5}", "Set volume", "audio"),
        cmd("call wifi.scan {}", "Trigger WiFi scan", "wifi"),
        cmd("call bluetooth.start_discovery {}", "Start BT scan", "bluetooth"),
        // ...
    ]
}
```

## Message display

### JSON formatting

Incoming JSON is pretty-printed with syntax highlighting:
- Keys in blue
- Strings in green
- Numbers in yellow
- Booleans in magenta
- Null in dim

Long messages are wrapped or truncated with expand-on-select (future).

### Direction markers

```
14:32:05 → subscribe battery.**
14:32:05 ← subscribe_ack {"pattern":"battery.**","available":true}
14:32:05 ← event battery.status {"percentage":85,"state":"discharging"}
14:32:10 ← event battery.status {"percentage":84,"state":"discharging"}
```

Timestamp + direction arrow + message type + condensed data.

## Crates

```toml
[dependencies]
ratatui = "0.28"
crossterm = { version = "0.29", features = ["event-stream"] }
tokio = { version = "1", features = ["rt-multi-thread", "net", "sync", "io-util", "macros"] }
tui-textarea = "0.7"
nucleo-matcher = "0.2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = "0.4"
anyhow = "1"
glimpse-types = { path = "../glimpse-types" }
```

## Crate: `glimpsectl`

```
glimpse/
├── glimpsectl/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs        # entry point, terminal setup, event loop
│       ├── app.rs         # App state, message handling
│       ├── ui.rs          # ratatui rendering (panes, modal)
│       ├── input.rs       # command parsing (short syntax → Request)
│       ├── picker.rs      # command picker modal with fuzzy search
│       └── catalog.rs     # all known commands/topics/methods
```

## Features

- Connect to glimpsed Unix socket
- Send requests in short syntax or raw JSON
- View incoming events with colored JSON
- Command picker with fuzzy search over all topics and methods
- Message log with timestamps and direction markers
- Auto-scroll with manual scroll override
- Connection status indicator
- Reconnect on daemon restart
- Command history (up/down in input to recall previous commands)
- Clear log
- Copy message to clipboard (future)
- Expand/collapse long JSON messages (future)
- Filter messages by topic (future)
