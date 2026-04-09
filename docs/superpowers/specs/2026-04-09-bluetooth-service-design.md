# Bluetooth Service — Design Spec

## Overview

Move Bluetooth out of applets and into one app-level service owned by `App`. The service owns BlueZ integration, state, discovery lifecycle, pairing agent, prompt routing, and reconnect behavior. Bluetooth applets become UI consumers that subscribe to service state and send typed commands.

This design is intentionally broader than the current applet-local provider refactor. The goal is to establish the pattern that later services like network, tray, and notifications can follow, while implementing only Bluetooth now.

## Architecture

```
glimpse-panel/src/
  app.rs                         — owns Services container
  services/
    mod.rs                       — Services, ServicesHandle
    bluetooth/
      mod.rs                     — public exports
      service.rs                 — BluetoothService worker + state machine
      agent.rs                   — org.bluez.Agent1 implementation
      protocol.rs                — service commands, events, prompt payloads
      dialogs.rs                 — GTK dialog controller/model for agent prompts

glimpse/src/
  providers/bluetooth.rs         — BlueZ DBus client methods + scans
  dbus/bluez.rs                  — BlueZ proxies
```

Boundaries:
- `BluetoothService` is app-level and global
- `BluetoothProvider` remains a BlueZ DBus client, not a UI/service orchestrator
- Bluetooth applets do not own providers, agents, or DBus connections
- Multiple Bluetooth applets may exist and all observe the same service state

## App-Level Services Container

`App` creates one `Services` container during init and passes a cloned `ServicesHandle` into panel construction. Panels pass typed service handles to applets that need them.

Minimal shape:

```rust
pub struct Services {
    pub bluetooth: BluetoothServiceHandle,
}

#[derive(Clone)]
pub struct ServicesHandle {
    pub bluetooth: BluetoothServiceHandle,
}
```

The handle is lightweight and cloneable. Applets only see typed handles, not global registries or string-based lookups.

## Bluetooth Service Responsibilities

`BluetoothService` is the single source of truth for:
- current Bluetooth snapshot
- discovery ownership
- BlueZ listener lifecycle
- BlueZ `Agent1` registration
- pending pairing prompt
- pending device actions
- service health and reconnect state

It owns:
- one system-bus `zbus::Connection` clone chain
- one `BluetoothProvider`
- one BlueZ listener task
- one BlueZ agent registration/task
- one broadcast/watch channel for current state
- one mpsc command channel for commands from applets/dialogs

It does not own:
- popover widgets
- applet-local transient UI state
- per-applet discovery/session state

## Bluetooth Provider Responsibilities

`BluetoothProvider` stays method-based. It is a BlueZ DBus client, not a service orchestrator.

Methods:
- `scan() -> BluetoothSnapshot`
- `listen(...) -> events`
- `set_powered(bool)`
- `start_discovery()`
- `stop_discovery()`
- `connect(address)`
- `disconnect(address)`
- `pair(address)`
- `trust(address, trusted: bool)`
- `forget(address)`

Provider additions required by this design:
- explicit `trust` support via `Device1.Trusted`
- no pairing UI logic
- no applet ownership assumptions

## Service State

The service publishes one state object to all subscribers.

```rust
pub struct BluetoothServiceState {
    pub health: BluetoothServiceHealth,
    pub snapshot: BluetoothSnapshot,
    pub prompt: Option<BluetoothPrompt>,
    pub active_action: Option<BluetoothActiveAction>,
}

pub enum BluetoothServiceHealth {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}
```

Rules:
- `snapshot` is last known good state
- transient failures do not clear snapshot to empty
- `prompt` is global; at most one prompt is active at a time
- `active_action` is informative UI state, not authoritative device state

## Commands

Applets send typed commands to the service.

```rust
pub enum BluetoothServiceCommand {
    SetPowered(bool),
    StartDiscovery,
    StopDiscovery,
    Connect { address: String },
    Disconnect { address: String },
    Pair { address: String },
    Trust { address: String, trusted: bool },
    Forget { address: String },
    PromptReply {
        id: BluetoothPromptId,
        reply: BluetoothPromptReply,
    },
}
```

Commands are app-level, not applet-local. Any Bluetooth applet may issue them.

## Discovery Model

Discovery is globally managed by the service, not by individual applets.

Claims:
- `initial` — startup discovery window used to populate device list quickly
- `popover_count` — number of currently open Bluetooth popovers across all panels/applets

Behavior:
- service startup starts `initial` discovery for 10 seconds
- opening a Bluetooth popover increments `popover_count`; transition 0 → 1 calls `StartDiscovery`
- closing a Bluetooth popover decrements `popover_count`; transition 1 → 0 calls `StopDiscovery`
- initial discovery expiry releases only the `initial` claim
- if both `initial` and popover discovery are active, BlueZ receives two `StartDiscovery` calls on the shared connection and must receive two matching `StopDiscovery` calls before this app fully releases discovery

This design supports multiple Bluetooth applets correctly.

## Pairing Agent

Port the old `glimpsed` Bluetooth agent into the panel app as a service-owned component.

Agent behavior:
- register one `org.bluez.Agent1` for the whole app at startup
- request default agent
- auto-confirm `RequestConfirmation`
- auto-authorize `AuthorizeService`
- if BlueZ requires user input, suspend the request and emit a prompt to the service state

Prompt types:

```rust
pub enum BluetoothPromptKind {
    Confirm { passkey: u32 },
    RequestPin,
    RequestPasskey,
    DisplayPin { pincode: String },
    DisplayPasskey { passkey: u32, entered: u16 },
}
```

Prompt payload:

```rust
pub struct BluetoothPrompt {
    pub id: BluetoothPromptId,
    pub device_path: String,
    pub device_label: String,
    pub kind: BluetoothPromptKind,
}
```

Reply payload:

```rust
pub enum BluetoothPromptReply {
    Confirm,
    Reject,
    Pin(String),
    Passkey(u32),
    Cancel,
}
```

Agent rule:
- auto-confirm first when BlueZ asks for confirmation/authorization
- if the flow still requires PIN/passkey/user entry, ask the applet via prompt state

## Dialog UX

Replace `zenity` with GTK dialogs owned by the panel app.

Dialog mockups:

Confirmation:

```text
Bluetooth Pairing

Pair with MX Keys Mini?
Code: 482931

[Cancel] [Pair]
```

PIN / passkey entry:

```text
Bluetooth Pairing

Enter PIN for Keychron K3

[ PIN input            ]
[Cancel] [Submit]
```

Display-only passkey:

```text
Bluetooth Pairing

Confirm this code on Pixel 8

482931

Waiting for device…
[Cancel]
```

Dialog rules:
- modal to the app window, not to a specific popover
- only one Bluetooth prompt dialog may be active at once
- dialogs outlive the originating popover; closing the popover must not cancel an in-flight pairing prompt
- dialog result sends `PromptReply` back to the service

## Applet / Service Protocol

Bluetooth applets are consumers of service state and emitters of typed commands.

Flow:

```text
user -> bluetooth applet -> BluetoothServiceCommand -> service -> provider/agent -> state broadcast -> bluetooth applet UI
```

Prompt flow:

```text
BlueZ Agent request
-> service receives agent callback
-> service stores BluetoothPrompt
-> service broadcasts state
-> applet/dialog shows prompt
-> user responds
-> applet sends PromptReply
-> service resolves pending agent request
-> service clears prompt
-> service broadcasts state
```

The applet must receive commands/events from the service. It does not poll or call BlueZ directly.

## Connect / Disconnect / Pair / Trust / Forget

Required support:
- `Connect`
- `Disconnect`
- `Pair`
- `Trust`
- `Forget`
- `SetPowered`
- `StartDiscovery`
- `StopDiscovery`

Behavior:
- `Pair` may trigger agent prompts
- after successful pair, service should set `Trusted=true` unless the device is already trusted
- `Trust` is also exposed as an explicit command for future UI
- `Forget` removes the device from BlueZ using the owning adapter path
- `Disconnect` and `Forget` must clear any matching pending action state in the service

## Crash / Reconnect / Recovery

The service manages failures centrally.

Failure sources:
- BlueZ connection errors
- listener stream termination
- agent registration failure
- method-call failures

Recovery model:
- keep last known good snapshot
- mark health as `Reconnecting { attempt }` or `Degraded`
- retry listener and agent setup with bounded backoff
- on successful recovery:
  - re-register agent
  - rebuild listener streams
  - refresh snapshot
  - publish `Ready`

Recovery rules:
- applets stay mounted during failures
- prompt state is cleared if the agent connection is lost
- discovery claims are reset on full service restart and then re-established from current service state if needed

## Logging

Human-oriented logs at service level:
- service start / stop / reconnect
- agent registered / unregistered / registration failed
- command requested / succeeded / failed
- prompt emitted / replied / cancelled
- scan summaries
- listener state changes

Avoid logging every per-signal detail at `info`; use `debug` for signal churn.

## App Integration

Changes at app/panel boundary:
- `App` creates `ServicesHandle`
- `setup_panels(...)` takes `services: ServicesHandle`
- `Panel::Init` includes `services`
- `create_applet(...)` receives `services`
- Bluetooth applet init becomes:

```rust
pub struct BluetoothInit {
    pub config: BluetoothConfig,
    pub service: BluetoothServiceHandle,
}
```

The Bluetooth applet no longer receives a DBus connection.

## Non-Goals

Not part of this spec:
- porting network/tray/notifications now
- redesigning the Bluetooth popover visuals
- background desktop notifications for pairing
- multiple concurrent pairing dialogs

## Migration Plan

1. Add app-level `Services` container with Bluetooth only.
2. Introduce `BluetoothService` and service handle.
3. Port `glimpsed` Bluetooth agent into `glimpse-panel` service layer.
4. Add provider `trust` support.
5. Move Bluetooth listener/discovery ownership from applet into service.
6. Convert Bluetooth applet to subscribe to service state and send service commands.
7. Add GTK pairing dialog controller wired to prompt state.
8. Remove remaining applet-local provider orchestration.

## Testing

Unit tests:
- discovery claim accounting
- prompt state transitions
- service reducer/state transitions
- provider helper logic

Integration tests:
- service command -> provider call routing
- pair flow with mocked agent prompt resolution
- reconnect path keeps last known snapshot

Manual checks:
- pair a device that auto-confirms
- pair a device requiring PIN/passkey
- trust / forget / connect / disconnect
- multiple Bluetooth applets share one service and one agent
