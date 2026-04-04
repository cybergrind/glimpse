# Notifications Provider

**Source:** freedesktop Notifications D-Bus (`org.freedesktop.Notifications`, session bus)

**What it does:** The daemon acts as the notification server — receives notifications from applications, stores history, provides DnD mode, and exposes notifications to clients. Alternatively can proxy to an existing notification daemon.

## System Interface

### org.freedesktop.Notifications (object: `/org/freedesktop/Notifications`)

The daemon registers this well-known name on the session bus to become the notification server.

Methods:
- `Notify(app_name: String, replaces_id: u32, app_icon: String, summary: String, body: String, actions: Vec<String>, hints: HashMap<String, Variant>, expire_timeout: i32) -> u32` — send notification; returns unique ID. `replaces_id` = 0 for new, >0 to update existing. `expire_timeout`: -1 = server default, 0 = never expire, >0 = milliseconds. `actions` are alternating [id, label] pairs.
- `CloseNotification(id: u32)` — dismiss notification
- `GetCapabilities() -> Vec<String>` — advertise server capabilities
- `GetServerInformation() -> (String, String, String, String)` — returns (name, vendor, version, spec_version)

Signals:
- `NotificationClosed(id: u32, reason: u32)` — 1=expired, 2=user dismissed, 3=closed via API, 4=undefined
- `ActionInvoked(id: u32, action_key: String)` — user clicked an action; "default" is reserved for primary action

### Hints

- `urgency: u8` — 0=low, 1=normal, 2=critical
- `category: String` — notification type (e.g. "im.received", "device.added", "transfer.complete")
- `desktop-entry: String` — .desktop file basename (for app icon lookup)
- `image-data: (i32, i32, i32, bool, i32, i32, Vec<u8>)` — raw pixel data (width, height, rowstride, has_alpha, bits_per_sample, channels, data)
- `image-path: String` — image file path or URI
- `sound-file: String` — audio file path
- `sound-name: String` — sound theme name (e.g. "message-new-email")
- `suppress-sound: bool`
- `transient: bool` — don't persist
- `resident: bool` — persist after action invocation
- `action-icons: bool` — use icons for actions
- `x: i32`, `y: i32` — positioning hints

### Capabilities

Strings the server can advertise via `GetCapabilities()`:
- `"actions"` — supports action buttons
- `"body"` — displays body text
- `"body-markup"` — body supports `<b>`, `<i>`, `<u>`, `<a>`, `<img>` HTML
- `"body-hyperlinks"` — body supports clickable links
- `"body-images"` — body supports inline images
- `"icon-static"` — single-resolution icons
- `"icon-multi"` — multi-resolution icons
- `"persistence"` — retains notifications beyond display timeout
- `"sound"` — plays notification sounds

### Registration as notification server

1. Claim `org.freedesktop.Notifications` on session bus (with `REPLACE_EXISTING` flag)
2. Serve object at `/org/freedesktop/Notifications`
3. Optionally install D-Bus service file at `~/.local/share/dbus-1/services/org.freedesktop.Notifications.service` for auto-activation

## Topics

- `notifications.status` — DnD state, unread count
- `notifications.list` — current active/pending notifications
- `notifications.history` — past dismissed notifications

## Methods

- `notifications.dismiss(id: u32)` — close a notification
- `notifications.dismiss_all()` — close all notifications
- `notifications.invoke_action(id: u32, action_key: String)` — trigger a notification action
- `notifications.set_dnd(enabled: bool)` — enable/disable Do Not Disturb
- `notifications.clear_history()` — clear notification history

## Types

```rust
/// Notification urgency level
enum Urgency {
    Low,
    Normal,
    Critical,
}

/// Why a notification was closed
enum CloseReason {
    Expired,
    UserDismissed,
    ApiCall,
    Undefined,
}

/// A notification
struct Notification {
    id: u32,
    app_name: String,
    app_icon: String,
    /// .desktop file basename for icon lookup
    desktop_entry: Option<String>,
    summary: String,
    body: String,
    urgency: Urgency,
    category: Option<String>,
    /// Action pairs: [(id, label)]
    actions: Vec<(String, String)>,
    /// Image path or URI (from image-path hint)
    image: Option<String>,
    /// Timestamp when received
    timestamp: u64,
    /// Whether notification persists after timeout
    resident: bool,
    /// Auto-expire timeout in milliseconds (0 = never)
    expire_timeout: i32,
}

/// Notification status, emitted on `notifications.status`
struct NotificationStatus {
    /// Do Not Disturb enabled
    dnd: bool,
    /// Number of unread/pending notifications
    count: u32,
}

/// Past notification, used in history
struct NotificationHistoryEntry {
    id: u32,
    app_name: String,
    app_icon: String,
    summary: String,
    body: String,
    urgency: Urgency,
    timestamp: u64,
    close_reason: CloseReason,
}
```

## Icons

- `preferences-system-notifications-symbolic` — notification settings
- `notifications-symbolic` — notification bell
- `notifications-disabled-symbolic` — DnD / notifications off
- `dialog-information-symbolic` — info notification
- `dialog-warning-symbolic` — warning notification
- `dialog-error-symbolic` — error notification

All icons above are available in Adwaita icon theme.

## Crates

- `zbus` (5) — D-Bus server implementation (claim bus name, serve interface, emit signals)

## Change Detection

**As the notification server, the daemon is the source of truth:**

- Incoming notifications arrive via `Notify()` method calls from applications
- Dismissals come from client method calls or auto-expire timers
- Action invocations come from client method calls
- DnD state is managed internally

No external change detection needed — the daemon owns all state.

**If proxying to an existing server instead:**
- Subscribe to `NotificationClosed` and `ActionInvoked` signals
- Cannot intercept incoming `Notify` calls (only the server sees them)

## Features

- Receive notifications from any application via standard D-Bus interface
- Notification display: summary, body (with optional markup), icon, actions
- Urgency levels (low, normal, critical)
- Auto-expire with configurable timeout
- Notification actions with callback support
- Image/icon support (file path, URI, raw pixel data)
- Sound hints support
- Notification history with configurable max size
- Do Not Disturb mode (queue notifications, suppress display)
- Dismiss individual or all notifications
- Clear history
- Resident notifications (persist after action)
- Category-based filtering
- App icon resolution via .desktop entry
- Notification grouping by application
- Notification replace/update support

## Notes

- The daemon must claim the bus name before any other notification daemon — conflicts with dunst, mako, swaync, etc.
- Only one notification server can run at a time on a session bus
- If acting as server, the daemon replaces any existing notification daemon
- History is not part of the freedesktop spec — it's a custom feature stored internally
- DnD is not standardized — implement as internal state with a custom property
- Critical urgency notifications should bypass DnD
- `image-data` hint contains raw RGBA — may need conversion for client display
- Actions array is flat: ["id1", "label1", "id2", "label2"] — parse as pairs
