# Power Provider

**Source:** logind D-Bus (`org.freedesktop.login1`, system bus) + PowerProfiles D-Bus (`net.hadess.PowerProfiles`, system bus)

**What it does:** System power actions (suspend, hibernate, reboot, shutdown, lock), power profile management, inhibitor locks, and scheduled shutdown.

## System Interface

### org.freedesktop.login1.Manager (object: `/org/freedesktop/login1`)

Methods — power actions (all take `interactive: bool`):
- `Suspend(bool)` — suspend to RAM
- `Hibernate(bool)` — suspend to disk
- `HybridSleep(bool)` — suspend to both RAM and disk
- `SuspendThenHibernate(bool)` — suspend to RAM, then hibernate after timeout
- `Reboot(bool)` — reboot system
- `PowerOff(bool)` — shut down system
- `Halt(bool)` — halt system

Methods — capability queries (return `String`: "yes", "no", "challenge", "na"):
- `CanSuspend() -> String`
- `CanHibernate() -> String`
- `CanHybridSleep() -> String`
- `CanSuspendThenHibernate() -> String`
- `CanReboot() -> String`
- `CanPowerOff() -> String`
- `CanHalt() -> String`

Methods — inhibitor locks:
- `Inhibit(what: String, who: String, why: String, mode: String) -> FileDescriptor` — acquire inhibitor lock; `what` is colon-separated list of: "shutdown", "sleep", "idle", "handle-power-key", "handle-suspend-key", "handle-hibernate-key", "handle-lid-switch"; `mode` is "block", "delay", or "block-weak"
- `ListInhibitors() -> Vec<(String, String, String, String, u32, u32)>` — returns (what, who, why, mode, uid, pid)

Methods — session control:
- `LockSession(session_id: String)` — lock a specific session
- `UnlockSession(session_id: String)` — unlock a specific session
- `LockSessions()` — lock all sessions

Methods — scheduled shutdown:
- `ScheduleShutdown(type: String, usec: u64)` — schedule shutdown; type is "poweroff", "reboot", or "halt"; usec is microseconds since UNIX epoch
- `CancelScheduledShutdown() -> bool` — returns whether a shutdown was cancelled

Methods — firmware:
- `SetRebootToFirmwareSetup(enable: bool)` — reboot into UEFI setup on next restart

Properties:
- `PreparingForSleep: bool` — true while sleep is in progress
- `PreparingForShutdown: bool` — true while shutdown is in progress
- `Docked: bool` — whether system is docked
- `IdleHint: bool` — whether system is idle
- `IdleSinceHint: u64` — microseconds since epoch (CLOCK_REALTIME)
- `IdleSinceHintMonotonic: u64` — microseconds (CLOCK_MONOTONIC)
- `BlockInhibited: String` — colon-separated list of active block inhibitors
- `DelayInhibited: String` — colon-separated list of active delay inhibitors

Signals:
- `PrepareForSleep(start: bool)` — emitted before (true) and after (false) sleep
- `PrepareForShutdown(start: bool)` — emitted before (true) and after (false) shutdown
- `SessionNew(session_id: String, session_path: ObjectPath)`
- `SessionRemoved(session_id: String, session_path: ObjectPath)`

### net.hadess.PowerProfiles (object: `/net/hadess/PowerProfiles`)

Properties:
- `ActiveProfile: String` (read/write) — "power-saver", "balanced", or "performance"
- `Profiles: Vec<HashMap<String, Variant>>` — available profiles, each with at least a "Profile" key
- `PerformanceDegraded: String` — reason performance is degraded (empty if not); e.g. "lap-detected", "high-operating-temperature"
- `PerformanceInhibited: String` — reason performance is inhibited (empty if not)
- `Actions: Vec<String>` — supported daemon actions

No methods — profile switching is done by setting `ActiveProfile` property.

Signals:
- `PropertiesChanged` (via `org.freedesktop.DBus.Properties`)

Valid profiles:
- `"power-saver"` — minimum power, reduced performance (always available)
- `"balanced"` — default mode (always available)
- `"performance"` — maximum performance (only on supported hardware)

## Topics

- `power.profiles` — available profiles and active profile
- `power.actions` — which power actions are available (CanSuspend, CanHibernate, etc.)
- `power.inhibitors` — active inhibitor locks

## Methods

- `power.set_profile(profile: PowerProfile)` — switch power profile
- `power.suspend()` — suspend to RAM
- `power.hibernate()` — suspend to disk
- `power.hybrid_sleep()` — suspend to both RAM and disk
- `power.reboot()` — reboot system
- `power.poweroff()` — shut down system
- `power.lock()` — lock current session
- `power.lock_all()` — lock all sessions
- `power.inhibit(what: Vec<InhibitWhat>, who: String, why: String, mode: InhibitMode) -> u64` — acquire inhibitor lock, returns handle ID
- `power.release_inhibit(handle: u64)` — release inhibitor lock by handle
- `power.schedule_shutdown(action: String, time_usec: u64)` — schedule shutdown/reboot; action is "poweroff", "reboot", or "halt"; time is microseconds since UNIX epoch
- `power.cancel_scheduled_shutdown() -> bool` — cancel scheduled shutdown, returns whether one was cancelled

## Types

```rust
/// Available power actions and whether the user can perform them
enum PowerActionAvailability {
    /// Supported and no authentication needed
    Yes,
    /// Available but user not permitted
    No,
    /// Available but requires authentication
    Challenge,
    /// Not available (hardware/kernel limitation)
    NotAvailable,
}

/// Power profile identifier
enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
}

/// Why performance mode is degraded or inhibited
struct PerformanceStatus {
    /// Empty if not degraded; e.g. "lap-detected", "high-operating-temperature"
    degraded_reason: String,
    /// Empty if not inhibited
    inhibited_reason: String,
}

/// Current power profiles state, emitted on `power.profiles`
struct PowerProfilesState {
    active: PowerProfile,
    available: Vec<PowerProfile>,
    performance_status: PerformanceStatus,
}

/// What a power action can do, emitted on `power.actions`
struct PowerActions {
    can_suspend: PowerActionAvailability,
    can_hibernate: PowerActionAvailability,
    can_hybrid_sleep: PowerActionAvailability,
    can_suspend_then_hibernate: PowerActionAvailability,
    can_reboot: PowerActionAvailability,
    can_poweroff: PowerActionAvailability,
}

/// What system activity an inhibitor blocks
enum InhibitWhat {
    Shutdown,
    Sleep,
    Idle,
    HandlePowerKey,
    HandleSuspendKey,
    HandleHibernateKey,
    HandleLidSwitch,
}

/// An active inhibitor lock
struct Inhibitor {
    what: Vec<InhibitWhat>,
    who: String,
    why: String,
    mode: InhibitMode,
    uid: u32,
    pid: u32,
}

enum InhibitMode {
    Block,
    Delay,
    BlockWeak,
}

/// Emitted on `power.inhibitors`
struct InhibitorList {
    inhibitors: Vec<Inhibitor>,
}
```

## Icons

Power actions:
- `system-shutdown-symbolic` — power off
- `system-reboot-symbolic` — reboot
- `system-lock-screen-symbolic` — lock screen
- `system-log-out-symbolic` — log out
- `system-suspend-symbolic` — suspend
- `system-hibernate-symbolic` — hibernate

Power profiles:
- `power-profile-power-saver-symbolic` — power saver mode
- `power-profile-balanced-symbolic` — balanced mode
- `power-profile-performance-symbolic` — performance mode

All icons above are available in Adwaita icon theme.

## Features

- Power actions: suspend, hibernate, hybrid sleep, suspend-then-hibernate, reboot, poweroff, halt
- Capability queries per action (yes/no/challenge/na)
- Power profile switching (power-saver, balanced, performance)
- Performance degradation/inhibition reason reporting
- Inhibitor lock management (acquire, release, list)
- Session locking (single session, all sessions)
- Scheduled shutdown with cancel support
- Reboot to firmware/UEFI setup
- PrepareForSleep/PrepareForShutdown signal handling (for save-state-before-sleep flows)
- Idle hint tracking

## Crates

- `zbus` (5) — D-Bus client for logind and PowerProfiles
- `logind-zbus` (0.1) — logind-specific zbus bindings (optional, can use raw zbus)

## Change Detection

**Power profiles:** `PropertiesChanged` D-Bus signal on `net.hadess.PowerProfiles`. Fires when active profile changes (including automatic changes due to thermal throttling).

**Sleep/shutdown:** `PrepareForSleep(bool)` and `PrepareForShutdown(bool)` signals on logind. Fires before (true) and after (false) sleep/shutdown — allows saving state before suspend.

**Inhibitors:** No dedicated signal. Must poll `ListInhibitors()` or watch for `PropertiesChanged` on `BlockInhibited`/`DelayInhibited` string properties.

**Session lock:** `Lock` / `Unlock` signals on `org.freedesktop.login1.Session` interface for individual sessions.

**Idle state:** `PropertiesChanged` on `IdleHint` property.

## Notes

- `interactive: false` skips polkit authentication prompts — daemon should use false and handle permission errors
- PowerProfiles may not be installed on all systems — provider should handle absence gracefully
- "performance" profile may not be available on all hardware
- Inhibitor locks are held via file descriptor — released when fd is closed
- ScheduleShutdown time is in microseconds since UNIX epoch, not seconds
