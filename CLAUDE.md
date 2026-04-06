# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Development build (default, loads CSS from files)
cargo build

# Production build (embeds CSS in binary)
cargo build --release --no-default-features

# Check compilation
cargo check

# Run the daemon
RUST_LOG=info cargo run -p glimpsed

# Run the panel (from glimpse-panel directory for CSS loading)
cd glimpse-panel && RUST_LOG=info cargo run

# Test with CLI
cargo run -p glimpsectl -- sub bluetooth.status
cargo run -p glimpsectl -- call bluetooth.set_powered '{"powered": true}'
cargo run -p glimpsectl -- inspect
```

## Architecture

Glimpse is a Wayland status panel ecosystem with a client-server architecture.

```
┌─────────────┐  NDJSON/Unix socket  ┌───────────────┐
│ glimpse-panel│ ◄─────────────────► │   glimpsed    │
│ (GTK4/relm4) │                     │  (daemon)     │
└─────────────┘                      │               │
┌─────────────┐                      │  ┌──────────┐ │
│ glimpsectl  │ ◄──────────────────► │  │ Broker   │ │
│ (CLI/TUI)   │                      │  └────┬─────┘ │
└─────────────┘                      │       │       │
                                     │  ┌────▼─────┐ │
    glimpse-client (shared library)  │  │Providers │ │
    glimpse-types  (shared protocol) │  │ audio    │ │
                                     │  │ battery  │ │
                                     │  │ bluetooth│ │
                                     │  │ power    │ │
                                     │  │ tray     │ │
                                     │  │ weather  │ │
                                     │  │ network  │ │
                                     │  └──────────┘ │
                                     └───────────────┘
```

### Workspace Crates

| Crate | Purpose |
|-------|---------|
| `glimpsed/` | System service daemon. Providers monitor Linux services (D-Bus, pactl, HTTP APIs) and emit events over Unix socket |
| `glimpse-panel/` | GTK4/relm4 Wayland panel. Layer-shell anchored, consumes daemon via `glimpse-client` |
| `glimpse-types/` | Shared NDJSON protocol types. `Request`/`Response` with `id: u64` for correlation |
| `glimpse-client/` | Async Rust client library. `Client::connect()`, `get()`, `subscribe()`, `call()` |
| `glimpsectl/` | CLI/TUI debug tool. `get`, `sub`, `call`, `inspect` commands with ratatui TUI |

### Protocol (NDJSON)

One JSON object per line over Unix socket (`$XDG_RUNTIME_DIR/glimpsed.sock`).

```
Client → Daemon:  {"type":"subscribe","data":{"pattern":"battery.**"},"id":1}
Daemon → Client:  {"type":"subscribe_ack","data":{"pattern":"battery.**","available":true},"id":1}
Daemon → Client:  {"type":"event","data":{"topic":"battery.status","ts":1234,"data":{...}},"id":1}
```

Request types: `Get`, `Subscribe`, `Unsubscribe`, `Call`
Response types: `GetResult`, `SubscribeAck`, `UnsubscribeAck`, `CallResult`, `Event`, `ProviderUnavailable`

### Daemon (glimpsed)

```
src/main.rs          — entry point, registers providers, binds Unix socket
src/broker.rs        — single-task message broker (no locks), routes events, manages provider lifecycle
src/server.rs        — per-client NDJSON reader/writer tasks
src/provider.rs      — Provider/ProviderFactory traits, ProviderRequest (Snapshot/Call via oneshot)
src/providers/       — system service providers
  dbus_props.rs      — DbusPropertyGroup helper (get/set/call/stream_changes/get_uncached)
  audio.rs           — pactl backend (status, outputs, inputs, streams)
  battery.rs         — UPower D-Bus (status, devices, charge threshold)
  bluetooth.rs       — BlueZ D-Bus via ObjectManager (adapters, devices, discovery)
  power.rs           — logind + PowerProfiles D-Bus (suspend, reboot, lock, profiles)
  tray.rs            — system-tray crate SNI watcher (items, menus, activation)
  weather.rs         — Open-Meteo HTTP API (current, hourly, forecast)
  ... and other
```

**Broker flow:**
1. Client connects → server spawns reader/writer tasks
2. Client subscribes → broker starts provider (lazy), registers subscription
3. Provider emits events → broker routes to matching subscribers
4. Client disconnects → broker removes subscriptions, stops unused providers

**Provider lifecycle:** lazy start on first subscriber, cancel token for clean shutdown, factory for restart after crash.

### Panel (glimpse-panel)

```
src/main.rs          — entry point, tracing, relm4 app
src/app.rs           — App component: layer shell, CSS providers, config/CSS hot-reload
src/config.rs        — TOML config from env/local/XDG paths
src/applets/         — panel applets (each: applet.rs, popover.rs, config.rs, mod.rs)
  audio/             — volume icon, sliders, device selectors, app streams
  battery/           — battery icon/label, popover with details + power profiles
  bluetooth/         — adaptive icon, device list with connect/disconnect, scan
  clock/             — time display, calendar, world clock
  power/             — battery + power profiles (scroll to change)
  session/           — username label, lock/logout/suspend/reboot/shutdown
  tray/              — SNI tray icons with menus
  weather/           — weather icon + temp, hourly/stats/10-day forecast
  ... and ither
panel.base.css       — base styles (embedded in prod, file-loaded in dev)
```

**Applet pattern:** each applet is a relm4 `Component` that subscribes to daemon topics via `sender.command()` + tokio, updates GTK widgets via messages. Popovers are separate `SimpleComponent`s.

### Configuration

Config is loaded from (in priority order):
1. `GLIMPSE_PANEL_CONFIG` env var
2. `./panel.toml`
3. `$XDG_CONFIG_HOME/glimpse/panel.toml`

Applets are configured in `[[panels]]` sections with per-applet `[applets.name]` overrides.

### Dev vs Prod Features

The `dev` feature (enabled by default) controls:
- Base CSS loaded from file vs embedded
- File watching for base CSS hot-reload

## Provider Development

### Key patterns

- Provider trait: `run()` returns `Pin<Box<dyn Future>>` for object safety
- Communication: `ProviderRequest` (Snapshot/Call) via oneshot channels to running provider tasks
- Events flow: provider → `mpsc::Sender<ProviderEvent>` → broker → clients
- Broker spawns snapshot and call requests as background tasks (never await inline — blocks all clients)
- Use `call_void` for D-Bus methods with no arguments, NOT `call::<_, ()>(method, &())` (wrong signature)
- D-Bus object paths (`o` type) need separate handling from strings (`s` type) — `String::try_from` fails on object paths
- Debounce with `tokio::time::sleep(Duration::from_secs(86400))` + reset, not short initial duration
- Parallel init with `tokio::join!` for independent async calls
- Human-readable `tracing::info!` for every method call: `"connecting to {address}"` not `method=%method`
- Require params explicitly: `let Some(val) = params["key"].as_bool()` not `unwrap_or(false)`

### D-Bus helpers

`DbusPropertyGroup` wraps a zbus proxy:
- `get` / `get_uncached` — read properties (uncached bypasses proxy cache after PropertiesChanged)
- `set` — write property
- `call` / `call_void` — method calls (call_void for no-return methods)
- `stream_changes` — stream PropertiesChanged signals

## Applet Development

### Key patterns

- Use `glib::spawn_future_local` for async calls from GTK signal handlers (NOT `tokio::spawn`)
- Persistent widget maps (`HashMap<key, WidgetRow>`) instead of clear+rebuild — preserves context menus, scroll position
- Click handlers capture state at creation time — use `Rc<Cell<bool>>` for mutable state read at click time
- Slider throttling: 100ms window with `Rc<Cell<Instant>>` to prevent ghosting
- Check `scale.state_flags().contains(ACTIVE)` before updating slider values to prevent snap-back
- Hero pattern: 32px icon + title + subtitle, consistent across all popover applets
- Box spacing is a widget property, not CSS — keep spacing values in Rust code
- GTK4 Switch: use `connect_state_set` with `Propagation::Stop`, guard with `Rc<Cell<bool>>` when updating programmatically
- PopoverMenu for right-click: GIO SimpleActionGroup + PopoverMenu from model
- Don't track external state (like BlueZ discovery) in UI — use local timers for button state

### CSS conventions

- All styles in `theme.css`, not hardcoded in Rust
- CSS variables: `--popover-padding`, `--popover-section-spacing`, `--dim-opacity`, `--accent-bg`
- Popover structure: `.foo-popover contents > box { margin: var(--popover-padding) }`
- Button text: always `font-weight: normal` on both button and label
- Tabular numbers: `font-variant-numeric: tabular-nums` for all numeric displays

# Important rules
- always use your built-in tools to read, search, grep, write files. Only use bash as the last resort
- if i don't allow you a certain actions - do not workaround it.
- do not write any code unless i ask you - print instead. I want to review, i want to type everything by hand
- make sure you don't hardcode styles in rust code, use theme.css for that


<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->

Use 'bd' for task tracking

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->
