# Sessions Provider

**Source:** logind D-Bus (`org.freedesktop.login1`, system bus)

**What it does:** Reports logged-in users, active sessions, seat information, and session type (wayland/x11/tty).

## System Interface

### org.freedesktop.login1.Manager (object: `/org/freedesktop/login1`)

Methods:
- `ListSessions() -> Vec<(String, u32, String, String, ObjectPath)>` — returns (session_id, uid, user_name, seat_id, session_path)
- `ListUsers() -> Vec<(u32, String, ObjectPath)>` — returns (uid, user_name, user_path)
- `ListSeats() -> Vec<(String, ObjectPath)>` — returns (seat_id, seat_path)
- `ListInhibitors() -> Vec<(String, String, String, String, u32, u32)>` — (what, who, why, mode, uid, pid)

Properties:
- `IdleHint: bool`
- `IdleSinceHint: u64` — microseconds since epoch

Signals:
- `SessionNew(session_id: String, session_path: ObjectPath)`
- `SessionRemoved(session_id: String, session_path: ObjectPath)`
- `UserNew(uid: u32, user_path: ObjectPath)`
- `UserRemoved(uid: u32, user_path: ObjectPath)`

### org.freedesktop.login1.Session (object: `/org/freedesktop/login1/session/{id}`)

Properties:
- `Id: String` — session ID
- `User: (u32, ObjectPath)` — (uid, user_path)
- `Name: String` — user name
- `Seat: (String, ObjectPath)` — (seat_id, seat_path)
- `Type: String` — "wayland", "x11", "tty", "mir", "unspecified"
- `Class: String` — "user", "greeter", "lock-screen"
- `Active: bool` — whether session is in foreground
- `State: String` — "online", "active", "closing"
- `Remote: bool` — whether session is remote (SSH, etc.)
- `RemoteHost: String` — remote host address (if remote)
- `RemoteUser: String` — remote user (if remote)
- `Service: String` — PAM service name
- `Desktop: String` — desktop environment name
- `Display: String` — X11 display or Wayland socket
- `TTY: String` — TTY device (if tty session)
- `IdleHint: bool`
- `LockedHint: bool`
- `Timestamp: u64` — session creation time (microseconds since epoch)

Signals:
- `Lock()` — session should be locked
- `Unlock()` — session should be unlocked

### org.freedesktop.login1.User (object: `/org/freedesktop/login1/user/{uid}`)

Properties:
- `UID: u32`
- `Name: String`
- `Sessions: Vec<(String, ObjectPath)>` — (session_id, session_path)
- `State: String` — "offline", "lingering", "online", "active", "closing"
- `Display: (String, ObjectPath)` — primary graphical session

## Topics

- `sessions.list` — all sessions with user, type, state
- `sessions.current` — current session details

## Methods

- `sessions.lock(session_id: String)` — lock a session
- `sessions.switch_user()` — switch to greeter/login screen
- `sessions.terminate(session_id: String)` — terminate a session

## Types

```rust
/// Session type
enum SessionType {
    Wayland,
    X11,
    Tty,
    Unspecified,
}

/// Session class
enum SessionClass {
    User,
    Greeter,
    LockScreen,
}

/// Session state
enum SessionState {
    Online,
    Active,
    Closing,
}

/// A login session
struct Session {
    id: String,
    uid: u32,
    user_name: String,
    seat_id: String,
    session_type: SessionType,
    session_class: SessionClass,
    state: SessionState,
    active: bool,
    remote: bool,
    remote_host: Option<String>,
    desktop: String,
    locked: bool,
    /// Session creation time (Unix timestamp)
    timestamp: u64,
}

/// Emitted on `sessions.list`
struct SessionList {
    sessions: Vec<Session>,
}

/// Emitted on `sessions.current`
struct CurrentSession {
    session: Session,
    /// Active inhibitors
    inhibitors: Vec<SessionInhibitor>,
}

/// An active inhibitor
struct SessionInhibitor {
    what: String,
    who: String,
    why: String,
    mode: String,
    uid: u32,
    pid: u32,
}
```

## Icons

- `system-users-symbolic` — users/sessions
- `user-available-symbolic` — active user

All icons above are available in Adwaita icon theme.

## Crates

- `zbus` (5) — D-Bus client for logind
- `logind-zbus` (0.1) — optional logind-specific bindings

## Change Detection

**Fully reactive via D-Bus signals:**
- `Manager.SessionNew` / `SessionRemoved` — sessions appear/disappear
- `Manager.UserNew` / `UserRemoved` — users log in/out
- `Session.PropertiesChanged` — session state, active, locked changes
- `Session.Lock` / `Unlock` — lock state changes

## Features

- List all active sessions with user, type, state
- Current session details (type, seat, desktop, display)
- Session type detection (Wayland, X11, TTY)
- Remote session detection (SSH)
- Lock/unlock session
- Switch user (back to greeter)
- Terminate sessions
- Inhibitor listing
- Multi-seat support
- Session creation time

## Notes

- Current session can be found via `$XDG_SESSION_ID` env var or by checking `Active=true`
- `Desktop` property matches `$XDG_CURRENT_DESKTOP` value
- Remote sessions (SSH) have `Remote=true` and `RemoteHost` set
- Greeter sessions have `Class=greeter` — filter them out for "user sessions" display
