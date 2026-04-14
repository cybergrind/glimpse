# Idle Daemon Design

## Goal

Add a Glimpse-owned idle daemon in the shared `glimpse` crate with:

- GNOME-style settings compatibility through `GSettings`
- a reusable shared API for reading, applying, and watching idle policy
- an app-level service handle that exposes runtime idle/lock state and inhibition

Out of scope for this feature:

- settings UI
- idle suppression applet UI
- a Glimpse-owned lock screen
- custom Glimpse-only idle policy schema

## Non-Goals

This feature does not replace the lock screen. It only manages idle policy and lock requests.

Authentication, secure lock surfaces, and unlock UI remain the responsibility of an external locker and the compositor. The daemon may request locking through `logind`, but it does not render or own the lock screen.

## Requirements

- Use existing GNOME settings keys as the source of truth when available.
- Integrate cleanly with `dconf`/`GSettings` on systems that provide GNOME desktop schemas.
- Expose a reusable library API that other Glimpse apps can use without touching backend details.
- Track actual session lock state through `logind`, not only lock requests.
- Support idle inhibition from Glimpse callers.
- Keep compositor-specific logic inside the shared backend, not in applets or app code.

## Settings Backend

### Source of Truth

The daemon uses existing GNOME-style keys through `gio::Settings`:

- `org.gnome.desktop.session::idle-delay`
- `org.gnome.desktop.screensaver::idle-activation-enabled`
- `org.gnome.desktop.screensaver::lock-enabled`
- `org.gnome.desktop.screensaver::lock-delay`

These keys are read and written directly. No Glimpse-specific idle settings schema is introduced in v1.

### Capability Detection

Not every system will have the required schemas installed. The settings layer must discover schemas through `SettingsSchemaSource` and expose per-key capabilities instead of failing globally.

If a schema or key is unavailable:

- the capability for that field is `false`
- the field still exists in snapshots with a safe default
- the daemon continues running in degraded or limited mode

### Apply Semantics

Grouped policy updates use delayed apply semantics so related changes are committed together. The settings layer never writes on startup. Writes happen only when a caller explicitly applies a policy.

### Fresh System Dependencies

On Arch, GNOME-style idle settings support requires:

- `glib2`
- `dconf`
- `gsettings-desktop-schemas`

## Architecture

The subsystem has three layers.

### `glimpse::idle::protocol`

Defines shared data types:

- settings capabilities
- persisted policy snapshot
- runtime daemon state
- inhibitor records
- commands and events

This is the only surface applets and apps should need to understand.

### `glimpse::idle::settings`

Reusable `GSettings` adapter responsible for:

- schema discovery
- capability reporting
- policy load/apply
- live settings change notifications

This layer contains no Wayland idle logic.

### `glimpse::idle::service`

Long-lived daemon/service that combines:

- persisted policy from `idle::settings`
- idle notifications from Wayland `ext-idle-notify-v1`
- lock state from `org.freedesktop.login1.Session`
- Glimpse-owned inhibitor handles

This service is created once by the app and exposed through a shared handle, following the same pattern as brightness, Bluetooth, and compositor listeners.

## Public API

### Settings API

```rust
pub struct IdleSettingsCapabilities {
    pub idle_delay: bool,
    pub idle_activation_enabled: bool,
    pub lock_enabled: bool,
    pub lock_delay: bool,
}

pub struct IdlePolicy {
    /// Mirrors org.gnome.desktop.session::idle-delay in seconds.
    pub idle_delay_seconds: u32,
    pub idle_activation_enabled: bool,
    pub lock_enabled: bool,
    /// Mirrors org.gnome.desktop.screensaver::lock-delay in seconds.
    pub lock_delay_seconds: u32,
}

pub struct IdleSettingsSnapshot {
    pub capabilities: IdleSettingsCapabilities,
    pub policy: IdlePolicy,
}

pub enum IdleSettingsEvent {
    Changed(IdleSettingsSnapshot),
}

pub struct IdleSettings;

impl IdleSettings {
    pub fn new() -> Self;
    pub fn capabilities(&self) -> &IdleSettingsCapabilities;
    pub fn load(&self) -> anyhow::Result<IdleSettingsSnapshot>;
    pub fn apply(&self, policy: &IdlePolicy) -> anyhow::Result<()>;
    pub async fn listen(
        &self,
        events: mpsc::Sender<IdleSettingsEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()>;
}
```

### Service API

```rust
pub enum IdleServiceHealth {
    Starting,
    Ready,
    Unsupported,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

pub enum IdleStage {
    Active,
    Idle,
    LockPending,
    Locked,
}

pub struct IdleInhibitorRecord {
    pub id: u64,
    pub who: String,
    pub why: String,
}

pub struct IdleServiceState {
    pub health: IdleServiceHealth,
    pub policy: IdleSettingsSnapshot,
    pub stage: IdleStage,
    pub is_idle: bool,
    pub is_locked: bool,
    pub active_inhibitors: Vec<IdleInhibitorRecord>,
}

pub enum IdleServiceCommand {
    Refresh,
    RequestLock,
    Inhibit { who: String, why: String },
    ReleaseInhibit { id: u64 },
}

pub struct IdleServiceHandle;

impl IdleServiceHandle {
    pub fn new(session: zbus::Connection) -> Self;
    pub fn subscribe(&self) -> watch::Receiver<IdleServiceState>;
    pub fn send(&self, command: IdleServiceCommand) -> Result<(), SendError<IdleServiceCommand>>;
}
```

The service owns runtime state. The settings adapter owns persisted policy access.

## Runtime Behavior

### Startup

1. Load the current `GSettings` policy snapshot.
2. Resolve the current session through `logind`.
3. Subscribe to `LockedHint` or `Lock`/`Unlock` session signals.
4. Start the Wayland idle listener if the compositor supports `ext-idle-notify-v1`.
5. Publish initial state.

### Idle Flow

1. If `idle-activation-enabled` is `false`, idle detection is effectively disabled.
2. If `idle-delay` is zero, the daemon treats idle detection as disabled.
3. When the idle threshold is reached:
   - `is_idle = true`
   - `stage = Idle`
4. If `lock-enabled` is `true` and there are no active inhibitors:
   - start a lock-delay timer
   - set `stage = LockPending`
5. When the lock-delay expires:
   - request lock through `logind`
6. When `logind` reports the session locked:
   - `is_locked = true`
   - `stage = Locked`
7. On user activity before lock:
   - cancel lock-pending timer
   - `is_idle = false`
   - `stage = Active`

### Manual Lock

`RequestLock` bypasses idle timers and requests an immediate session lock through `logind`.

### Inhibition

Any active inhibitor suppresses idle-triggered lock escalation. Inhibitors do not suppress an explicit `RequestLock`.

Inhibitors are process-local handles owned by Glimpse callers. v1 does not attempt to expose a cross-process inhibitor registry.

## Backend Strategy

### Wayland Idle Detection

Use `ext-idle-notify-v1` for reactive idle/resume events.

If the compositor does not support the protocol:

- the daemon still exposes settings and lock state
- health becomes degraded or unsupported for idle detection
- idle-triggered timers do not run

This is a deliberate capability model. Applets consume typed state rather than branching on compositor.

### Lock State

Use `org.freedesktop.login1.Session` for:

- lock request
- lock/unlock signals
- `LockedHint`

The daemon must not treat “lock requested” as equivalent to “locked”.

## App Integration

`glimpse/src/services/mod.rs` gains:

- `idle: IdleServiceHandle`

Consumers use the service handle only. They do not read `gio::Settings`, call `login1`, or open Wayland idle protocol objects directly.

This keeps the backend swappable and aligns with the shared-service architecture already used in Glimpse.

## Error Handling

- Missing GNOME schemas: supported with reduced capabilities.
- Missing Wayland idle protocol: supported with no reactive idle detection.
- `logind` unavailable: service enters degraded state; lock requests fail explicitly.
- Settings write failures: returned to caller with key-specific context.
- Listener failures: service transitions through reconnecting/degraded states and republishes state.

The service must prefer explicit degraded state over panics or silent no-op behavior.

## Testing

### Unit Tests

- schema capability detection
- `IdlePolicy` load/apply mappings to GNOME keys
- idle-delay and lock-delay normalization
- inhibitor add/release behavior
- stage transitions for idle, resume, lock-pending, and locked

### Service Tests

- settings change updates state
- lock request updates state only after simulated lock signal
- inhibition suppresses idle-triggered lock
- unsupported backend yields stable degraded state

### Integration Checks

- system with `gsettings-desktop-schemas` and `dconf`
- compositor with `ext-idle-notify-v1`
- active `logind` session

## Future Work

The following are intentionally deferred:

- settings UI
- idle suppression applet
- multi-stage custom policy beyond GNOME keys
- sleep orchestration hooks
- external idle daemon interoperability mode
- external locker supervision and readiness tracking
- system-wide idle inhibitors beyond Glimpse-owned handles

## Recommendation

Implement the feature as a Glimpse-owned idle daemon with GNOME-style settings compatibility and a shared service handle.

That gives immediate reuse across apps, clean integration with existing desktop settings, and avoids mixing secure locker responsibilities into the daemon.
