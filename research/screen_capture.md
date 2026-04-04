# Screen Capture Provider

**Source:** XDG Desktop Portal D-Bus (`org.freedesktop.portal.Desktop`, session bus)

**What it does:** Takes screenshots and manages screencasts via the portal interface. Reports active screen sharing sessions.

## System Interface

### org.freedesktop.portal.Screenshot (object: `/org/freedesktop/portal/desktop`)

Methods:
- `Screenshot(parent_window: String, options: HashMap<String, Variant>) -> ObjectPath` — take screenshot; options: `modal` (bool), `interactive` (bool, let user select area). Returns request path.
- `PickColor(parent_window: String, options: HashMap<String, Variant>) -> ObjectPath` — pick a color from screen. Response contains `color: (f64, f64, f64)` RGB.

Response (via `org.freedesktop.portal.Request.Response` signal):
- `uri: String` — file:// URI of saved screenshot

### org.freedesktop.portal.ScreenCast (object: `/org/freedesktop/portal/desktop`)

Methods:
- `CreateSession(options: HashMap<String, Variant>) -> ObjectPath` ��� create screencast session
- `SelectSources(session: ObjectPath, options: HashMap<String, Variant>) -> ObjectPath` — select what to share; options: `types` (u32: 1=monitor, 2=window, 4=virtual), `multiple` (bool), `cursor_mode` (u32: 1=hidden, 2=embedded, 4=metadata)
- `Start(session: ObjectPath, parent_window: String, options: HashMap<String, Variant>) -> ObjectPath` — start sharing. Response contains `streams: Vec<(u32, HashMap<String, Variant>)>` with PipeWire node IDs.

Session lifecycle: CreateSession → SelectSources → Start → stream PipeWire nodes → close session.

### Portal Request/Response pattern

All portal methods return a `Request` object path. Listen for `Response(response: u32, results: HashMap<String, Variant>)` signal:
- response 0 = success
- response 1 = user cancelled
- response 2 = other error

## Topics

- `screen_capture.status` — whether any screen sharing sessions are active
- `screen_capture.recordings` — active screencast sessions

## Methods

- `screen_capture.screenshot(interactive: bool) -> String` — take screenshot, returns file URI
- `screen_capture.pick_color() -> (f64, f64, f64)` — pick color from screen, returns RGB
- `screen_capture.start_screencast(source_type: SourceType) -> String` — start screencast, returns session ID
- `screen_capture.stop_screencast(session_id: String)` — stop screencast

## Types

```rust
/// What to capture
enum SourceType {
    Monitor,
    Window,
    Virtual,
}

/// Cursor visibility in screencast
enum CursorMode {
    Hidden,
    Embedded,
    Metadata,
}

/// An active screencast session
struct ScreencastSession {
    id: String,
    source_type: SourceType,
    /// PipeWire node ID for the stream
    pipewire_node: u32,
}

/// Screen capture status, emitted on `screen_capture.status`
struct ScreenCaptureStatus {
    /// Number of active screencast sessions
    active_sessions: u32,
    /// Whether any screen is being shared
    is_sharing: bool,
}
```

## Icons

- `screen-shared-symbolic` — screen sharing active (if available)
- `screenshot-recorded-symbolic` — screenshot taken (if available)
- `camera-photo-symbolic` — screenshot action
- `media-record-symbolic` — screencast recording

## Crates

- `zbus` (5) — D-Bus client for portal interfaces
- `ashpd` — high-level Rust bindings for XDG Desktop Portals (Screenshot, ScreenCast, etc.)

## Change Detection

**Portal sessions:** Track session creation/destruction internally. The daemon manages sessions and knows when they start/stop.

**Privacy indicator:** When a screencast session is active, emit `screen_capture.status` with `is_sharing=true`. Useful for privacy indicators.

## Features

- Take screenshots (full screen or interactive region selection)
- Color picker from screen
- Start/stop screencasts
- Source type selection (monitor, window, virtual)
- Cursor mode control
- PipeWire stream node for screencast consumers
- Active session tracking for privacy indicators
- Portal permission handling (user consent dialog)

## Notes

- Portal methods show a user consent dialog — cannot be done silently
- Screenshots are saved to a temporary file, URI returned to caller
- Screencasts produce PipeWire streams — clients need PipeWire to consume the video
- `ashpd` crate provides typed, async-friendly wrappers around all portal interfaces
- `parent_window` parameter can be empty string if no parent window context
- Portal availability depends on the compositor's portal backend (xdg-desktop-portal-gnome, xdg-desktop-portal-wlr, etc.)
- Active screencast sessions feed into the privacy provider for screen-sharing-in-use indicators
