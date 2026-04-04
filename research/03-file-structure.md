# glimpsed — File Structure

```
glimpse/
├── Cargo.toml                          # workspace root
├── glimpse-types/                      # shared JSON message types
│   ├── Cargo.toml                      # serde, serde_json
│   └── src/
│       └── lib.rs                      # Request, Response, RequestResult
├── glimpsed/                           # daemon binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                     # entry point, signal handling, socket bind
│       ├── server.rs                   # accept loop, per-client reader/writer tasks
│       ├── broker.rs                   # subscription management, event routing
│       ├── pattern.rs                  # wildcard pattern matching
│       └── providers/
│           ├── mod.rs                  # Provider trait, ProviderFactory, registry
│           ├── dbus_props.rs           # DbusPropertyGroup helper
│           ├── debug.rs               # test provider (periodic events)
│           ├── battery.rs             # UPower D-Bus
│           ├── power.rs               # logind + PowerProfiles D-Bus
│           ├── brightness.rs          # /sys/class/backlight + ddcutil + iio-sensor
│           ├── display.rs             # compositor IPC + DRM/KMS
│           ├── audio.rs               # PulseAudio D-Bus / PipeWire
│           ├── nightlight.rs          # wlr-gamma-control + GeoClue2
│           ├── wifi.rs                # NetworkManager D-Bus (wireless)
│           ├── bluetooth.rs           # BlueZ D-Bus (via bluer)
│           ├── keyboard.rs            # compositor IPC
│           ├── workspaces.rs          # compositor IPC
│           ├── notifications.rs       # freedesktop Notifications (server)
│           ├── tray.rs                # StatusNotifierItem/Watcher + DBusMenu
│           ├── theme.rs               # XDG portal + gsettings
│           ├── weather.rs             # Open-Meteo HTTP API
│           ├── system_stats.rs        # /proc, /sys, statvfs
│           ├── airplane.rs            # rfkill /dev/rfkill + sysfs
│           ├── mpris.rs               # MPRIS2 D-Bus
│           ├── apps.rs                # .desktop file indexing
│           ├── clipboard.rs           # Wayland data control protocol
│           ├── idle.rs                # ext-idle-notify + logind
│           ├── locale.rs              # timedate1 D-Bus + /etc/locale.conf
│           ├── sessions.rs            # logind sessions D-Bus
│           ├── network.rs             # NetworkManager D-Bus (ethernet, VPN)
│           ├── removable_media.rs     # udisks2 D-Bus
│           ├── geolocation.rs         # GeoClue2 D-Bus
│           ├── screen_capture.rs      # XDG Desktop Portal
│           ├── accessibility.rs       # gsettings a11y schemas
│           ├── printers.rs            # CUPS CLI/HTTP
│           ├── camera.rs              # PipeWire camera nodes
│           ├── privacy.rs             # meta-provider (aggregates audio/camera/screen)
│           ├── clock.rs               # system clock + chrono
│           ├── calendar.rs            # CalDAV + GOA D-Bus
│           └── compositor/
│               ├── mod.rs             # CompositorBackend trait + autodetect
│               ├── niri.rs            # niri IPC (niri-ipc crate)
│               └── hyprland.rs        # hyprland IPC (hyprland crate)
└── glimpse-client/                    # async client library
    ├── Cargo.toml
    └── src/
        └── lib.rs                     # connect, subscribe->Stream, get, call, auto-reconnect
```

## Workspace Dependencies

Existing:
- `tokio`, `zbus`, `futures-util`, `serde`, `serde_json`, `tracing`, `tracing-subscriber`, `chrono`, `chrono-tz`

New:
- `tokio-util` (0.7) — CancellationToken
- `anyhow` (1) — error handling
- `tokio-stream` (0.1) — Stream adapters
- `bytes` (1) — buffer management
- `reqwest` (0.12) — HTTP client (weather)
- `nix` (0.29) — POSIX syscalls (rfkill, signals, statvfs)
- `inotify` — Linux inotify (sysfs watching)
- `wayland-client` — Wayland client connection
- `wayland-protocols-wlr` (0.3) — wlr protocol bindings
- `bluer` (0.16) — BlueZ Rust bindings
- `niri-ipc` — niri compositor IPC
- `hyprland` (0.4) — hyprland compositor IPC
- `sysinfo` — system stats
- `procfs` (0.17) — /proc parsing
- `wl-clipboard-rs` — Wayland clipboard
- `freedesktop-desktop-entry` — .desktop file parsing
- `freedesktop-icons` — icon theme lookup
- `ashpd` — XDG Desktop Portal bindings
- `sunrise-sunset-calculator` — solar position algorithm
- `libpulse-binding` (2.30) — PulseAudio bindings
- `mpris` — MPRIS2 bindings
- `system-tray` (0.8.5) — StatusNotifierItem (already in workspace)

Removed (no longer needed):
- ~~`prost`~~ — was for protobuf
- ~~`prost-types`~~ — was for protobuf
- ~~`prost-build`~~ — was for protobuf build step
