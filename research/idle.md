# Idle Provider

**Source:** ext-idle-notify-v1 Wayland protocol, logind D-Bus for screen lock

**What it does:** Tracks user idle time, reports screen lock/unlock events, and provides idle inhibition.

## System Interface

### ext-idle-notify-v1 (Wayland protocol)

Protocol for receiving idle/resume notifications. Supported by: Sway, Hyprland, Niri, River, COSMIC.

Flow:
1. Bind to `ext_idle_notifier_v1` global
2. Call `get_idle_notification(timeout_msec, seat)` — creates a notification object
3. Receive `idled` event when user has been idle for the specified duration
4. Receive `resumed` event when user becomes active again

Multiple notifications with different timeouts can be created simultaneously (e.g. dim screen at 5min, lock at 10min).

### org.freedesktop.login1.Session (logind, system bus)

For screen lock state:

Methods:
- `Lock()` — lock this session
- `Unlock()` — unlock (typically called by lock screen app)
- `SetIdleHint(idle: bool)` — set idle hint for this session

Properties:
- `IdleHint: bool` (RO) — whether session is idle
- `IdleSinceHint: u64` (RO) — idle since timestamp (microseconds, CLOCK_REALTIME)
- `LockedHint: bool` (RO) — whether session is locked

Signals:
- `Lock()` — session should be locked (compositor/lock screen responds to this)
- `Unlock()` — session should be unlocked

### Idle inhibition

**Wayland protocol:** `idle-inhibit-unstable-v1` — a Wayland client can create an inhibitor object tied to a surface. While the inhibitor exists, the compositor won't consider the user idle.

**logind:** `Inhibit("idle", who, why, "block")` — acquire an idle inhibitor lock (see power provider).

## Topics

- `idle.status` — idle state, idle duration, locked state
- `idle.screen_lock` — lock/unlock events

## Methods

- `idle.lock_screen()` — request screen lock via logind
- `idle.inhibit(who: String, why: String) -> u64` — acquire idle inhibitor, returns handle
- `idle.release_inhibit(handle: u64)` — release inhibitor

## Types

```rust
/// Current idle state, emitted on `idle.status`
struct IdleStatus {
    /// Whether the user is currently idle
    is_idle: bool,
    /// How long the user has been idle (None if not idle)
    idle_duration: Option<Duration>,
    /// Whether the screen is locked
    is_locked: bool,
    /// Number of active idle inhibitors
    inhibitor_count: u32,
}
```

## Icons

- `system-lock-screen-symbolic` — screen lock
- `preferences-desktop-screensaver-symbolic` — idle/screensaver settings

All icons above are available in Adwaita icon theme.

## Crates

- `wayland-client` — for ext-idle-notify-v1 protocol
- `wayland-protocols` — protocol definitions (ext-idle-notify is in staging)
- `zbus` (5) — logind Session interface for lock/unlock

## Change Detection

**Idle state:** `idled` / `resumed` events from ext-idle-notify-v1. Fully reactive.

**Lock state:** logind `Lock` / `Unlock` signals on Session object, or `PropertiesChanged` for `LockedHint`. Fully reactive.

## Features

- User idle detection with configurable timeout
- Idle duration tracking
- Screen lock/unlock event reporting
- Lock screen triggering via logind
- Idle inhibition (prevent idle during video playback, presentations)
- Multiple idle thresholds (dim, lock, suspend)
- DPMS state awareness (screen on/off)
- Inhibitor listing and management

## Notes

- The daemon needs a Wayland connection for ext-idle-notify-v1
- Multiple idle notifications at different timeouts are useful (e.g. 5min dim, 10min lock, 30min suspend)
- logind `Lock` signal is how compositors know to activate their lock screen
- Idle inhibition via Wayland protocol is per-surface (requires a visible window) — logind inhibitors are more flexible for a daemon
