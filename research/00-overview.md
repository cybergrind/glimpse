# glimpsed — Overview

A system service daemon that provides centralized access to Linux desktop services over a Unix socket using protobuf. Instead of each client (panel, CLI, widget) talking directly to D-Bus services, PipeWire, Wayland protocols, and /proc — they all talk to glimpsed.

## Why

- **Single connection** — multiple clients share one D-Bus connection per service
- **Pub/sub** — clients subscribe to topics and receive live updates
- **Lazy providers** — only connect to a service when someone is subscribed
- **Abstraction** — clients don't need to know if audio comes from PipeWire or PulseAudio, or if workspaces come from niri or hyprland
- **Compositor-agnostic** — one client API works across niri, hyprland, sway

## Protocol

- Socket: `$XDG_RUNTIME_DIR/glimpsed.sock`
- Framing: 4-byte big-endian length prefix + protobuf message
- Request types: `Get` (one-shot read), `Subscribe` (live stream), `MethodCall` (action)
- Response types: `GetResult`, `SubscribeAck`, `Event`, `MethodResult`
- Wildcard patterns: `*` matches one segment, `**` matches any depth

## Providers

32 providers covering:

| Category | Providers |
|----------|-----------|
| Power | battery, power |
| Display | brightness, display, nightlight |
| Audio | audio, mpris |
| Connectivity | wifi, bluetooth, network, airplane |
| Desktop | workspaces, keyboard, theme, tray, notifications, apps, clipboard |
| System | system_stats, sessions, idle, locale, clock, camera, printers |
| Location | geolocation, weather, calendar |
| Privacy | privacy, screen_capture, accessibility |
| Storage | removable_media |

## Workspace Crates

- `glimpse-proto` — shared protobuf types
- `glimpsed` — daemon binary
- `glimpse-client` — async Rust client library
