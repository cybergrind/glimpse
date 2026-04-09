# Bluetooth Service Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Bluetooth to one app-level service with a shared BlueZ agent, typed state/command APIs, GTK pairing dialogs, and applet consumers that no longer own provider lifecycle.

**Architecture:** `App` owns a global `Services` container and passes a `BluetoothServiceHandle` into Bluetooth applets. `BluetoothService` owns the provider, BlueZ listener, discovery claims, agent registration, prompt state, and reconnect behavior. Bluetooth applets subscribe to service state and send typed commands only.

**Tech Stack:** Rust, Relm4, GTK4, Tokio, zbus, BlueZ D-Bus

---

## File Structure

### Create
- `glimpse-panel/src/services/mod.rs` — app-level services container and typed handles
- `glimpse-panel/src/services/bluetooth/mod.rs` — Bluetooth service exports
- `glimpse-panel/src/services/bluetooth/protocol.rs` — commands, events, prompts, health, state
- `glimpse-panel/src/services/bluetooth/service.rs` — Bluetooth service worker, reconnect loop, discovery claims
- `glimpse-panel/src/services/bluetooth/agent.rs` — `org.bluez.Agent1` implementation and prompt bridge
- `glimpse-panel/src/services/bluetooth/dialogs.rs` — GTK dialog controller/model for pairing prompts

### Modify
- `glimpse-panel/src/app.rs` — create and hold `ServicesHandle`, pass into panels
- `glimpse-panel/src/panels/component.rs` — accept and forward `ServicesHandle`
- `glimpse-panel/src/panels/mod.rs` — export updated `Init`
- `glimpse-panel/src/applets/mod.rs` — inject `BluetoothServiceHandle` into Bluetooth applet init
- `glimpse-panel/src/applets/bluetooth/applet.rs` — replace provider ownership with service subscription + commands
- `glimpse-panel/src/applets/bluetooth/popover.rs` — add prompt/dialog integration hooks if needed
- `glimpse-panel/src/applets/bluetooth/components/hero.rs` — consume service health/prompt-driven status text
- `glimpse/src/dbus/bluez.rs` — add `Trusted` setter/getter support as needed
- `glimpse/src/providers/bluetooth.rs` — add `trust(address, trusted)` and keep provider method-based

### Test
- `glimpse-panel/src/services/bluetooth/service.rs` unit tests
- `glimpse-panel/src/services/bluetooth/agent.rs` unit tests
- existing Bluetooth applet tests in `glimpse-panel/src/applets/bluetooth/...`
- `glimpse/src/providers/bluetooth.rs` unit tests

---

### Task 1: Add App-Level Services Injection

**Files:**
- Create: `glimpse-panel/src/services/mod.rs`
- Modify: `glimpse-panel/src/app.rs`
- Modify: `glimpse-panel/src/panels/component.rs`
- Modify: `glimpse-panel/src/panels/mod.rs`
- Modify: `glimpse-panel/src/applets/mod.rs`
- Test: `cargo check -p glimpse-panel`

- [ ] **Step 1: Write the failing compile change**

Add a new `services` module import in `glimpse-panel/src/app.rs` and thread a `ServicesHandle` parameter through `setup_panels(...)`, `panels::Init`, and `create_applet(...)`.

```rust
use crate::services::{Services, ServicesHandle};

fn setup_panels(
    config: &Config,
    dbus: zbus::Connection,
    system: zbus::Connection,
    client: Option<Arc<Client>>,
    services: ServicesHandle,
) -> Vec<Controller<panels::Panel>> { /* ... */ }
```

- [ ] **Step 2: Run check to verify it fails**

Run: `cargo check -p glimpse-panel`
Expected: FAIL with unresolved `services` module / missing fields on `panels::Init` / mismatched `create_applet(...)` arguments

- [ ] **Step 3: Write minimal services container**

Create `glimpse-panel/src/services/mod.rs`:

```rust
pub mod bluetooth;

#[derive(Clone)]
pub struct ServicesHandle {
    pub bluetooth: bluetooth::BluetoothServiceHandle,
}

pub struct Services {
    pub handle: ServicesHandle,
}

impl Services {
    pub fn new(system: zbus::Connection) -> Self {
        let bluetooth = bluetooth::BluetoothServiceHandle::new_placeholder(system);
        Self {
            handle: ServicesHandle { bluetooth },
        }
    }
}
```

Then update `app.rs`, `panels/component.rs`, and `applets/mod.rs` to pass `services.clone()`.

- [ ] **Step 4: Run check to verify it passes**

Run: `cargo check -p glimpse-panel`
Expected: PASS or FAIL only on the next intentionally missing Bluetooth service API pieces

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/services/mod.rs glimpse-panel/src/app.rs glimpse-panel/src/panels/component.rs glimpse-panel/src/panels/mod.rs glimpse-panel/src/applets/mod.rs
git commit -m "refactor: inject app-level services into panels"
```

---

### Task 2: Define Bluetooth Service Protocol

**Files:**
- Create: `glimpse-panel/src/services/bluetooth/protocol.rs`
- Create: `glimpse-panel/src/services/bluetooth/mod.rs`
- Test: `cargo test -p glimpse-panel bluetooth_prompt_protocol_roundtrip -- --nocapture`

- [ ] **Step 1: Write the failing test**

Add a unit test in `protocol.rs` that constructs the command/state/prompt types and verifies the prompt id and state clone behavior are stable.

```rust
#[test]
fn bluetooth_prompt_protocol_roundtrip() {
    let state = BluetoothServiceState {
        health: BluetoothServiceHealth::Starting,
        snapshot: BluetoothSnapshot::default(),
        prompt: Some(BluetoothPrompt {
            id: BluetoothPromptId(7),
            device_path: "/org/bluez/hci0/dev_AA_BB".into(),
            device_label: "Headphones".into(),
            kind: BluetoothPromptKind::RequestPin,
        }),
        active_action: None,
    };

    assert_eq!(state.prompt.unwrap().id.0, 7);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel bluetooth_prompt_protocol_roundtrip -- --nocapture`
Expected: FAIL because `BluetoothServiceState`, `BluetoothPrompt`, or `BluetoothPromptId` do not exist

- [ ] **Step 3: Write minimal protocol types**

Create `protocol.rs`:

```rust
use glimpse::providers::bluetooth::BluetoothSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BluetoothPromptId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothPromptKind {
    Confirm { passkey: u32 },
    RequestPin,
    RequestPasskey,
    DisplayPin { pincode: String },
    DisplayPasskey { passkey: u32, entered: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothPrompt {
    pub id: BluetoothPromptId,
    pub device_path: String,
    pub device_label: String,
    pub kind: BluetoothPromptKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothPromptReply {
    Confirm,
    Reject,
    Pin(String),
    Passkey(u32),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothServiceHealth {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothActiveAction {
    SetPowered(bool),
    Connect { address: String },
    Disconnect { address: String },
    Pair { address: String },
    Trust { address: String, trusted: bool },
    Forget { address: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothServiceState {
    pub health: BluetoothServiceHealth,
    pub snapshot: BluetoothSnapshot,
    pub prompt: Option<BluetoothPrompt>,
    pub active_action: Option<BluetoothActiveAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothServiceCommand {
    SetPowered(bool),
    StartDiscovery,
    StopDiscovery,
    Connect { address: String },
    Disconnect { address: String },
    Pair { address: String },
    Trust { address: String, trusted: bool },
    Forget { address: String },
    PromptReply { id: BluetoothPromptId, reply: BluetoothPromptReply },
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p glimpse-panel bluetooth_prompt_protocol_roundtrip -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/services/bluetooth/protocol.rs glimpse-panel/src/services/bluetooth/mod.rs
git commit -m "feat: add bluetooth service protocol types"
```

---

### Task 3: Add Provider Trust Support

**Files:**
- Modify: `glimpse/src/dbus/bluez.rs`
- Modify: `glimpse/src/providers/bluetooth.rs`
- Test: `glimpse/src/providers/bluetooth.rs`

- [ ] **Step 1: Write the failing test**

Add a unit test in `glimpse/src/providers/bluetooth.rs` that verifies trusted state is preserved in device snapshots and that the `BluetoothDevice` model supports trust as a first-class capability.

```rust
#[test]
fn snapshot_counts_trusted_device_without_affecting_connected_count() {
    let snapshot = BluetoothSnapshot::new(
        vec![adapter("/org/bluez/hci0", true, false)],
        vec![BluetoothDevice {
            address: "AA:BB:CC:DD:EE:FF".into(),
            name: "Device".into(),
            device_type: BluetoothDeviceType::Unknown,
            paired: true,
            connected: false,
            trusted: true,
            battery: None,
            rssi: None,
            adapter: "/org/bluez/hci0".into(),
        }],
    );

    assert!(snapshot.devices[0].trusted);
    assert_eq!(snapshot.status.connected_count, 0);
}
```

- [ ] **Step 2: Run test to verify it fails or is missing coverage**

Run: `cargo test -p glimpse snapshot_counts_trusted_device_without_affecting_connected_count -- --nocapture`
Expected: PASS if model already covers it, which means the next red test must be the missing provider API

- [ ] **Step 3: Write the failing provider API test**

Add a compile-target test or use the code step as the red/green pair for the missing API:

```rust
// call site to add during implementation
provider.trust("AA:BB:CC:DD:EE:FF", true).await?;
```

Run:
`cargo check -p glimpse`
Expected: FAIL because `BluetoothProvider::trust` and/or `Device1Proxy::set_trusted` do not exist

- [ ] **Step 4: Write minimal trust implementation**

Add to `glimpse/src/dbus/bluez.rs`:

```rust
#[zbus(property)]
fn set_trusted(&self, value: bool) -> zbus::Result<()>;
```

Add to `glimpse/src/providers/bluetooth.rs`:

```rust
pub async fn trust(&self, address: &str, trusted: bool) -> anyhow::Result<()> {
    let device = self.resolve_device(address).await?;
    tracing::info!(
        address = %device.address,
        name = %device.name,
        trusted,
        "bluetooth: trust requested"
    );
    let proxy = self.device_proxy(&device.path).await?;
    proxy
        .set_trusted(trusted)
        .await
        .with_context(|| format!("failed to set trust for {}", device.address))?;
    tracing::info!(address = %device.address, trusted, "bluetooth: trust succeeded");
    Ok(())
}
```

- [ ] **Step 5: Run verification**

Run:
- `cargo test -p glimpse snapshot_counts_trusted_device_without_affecting_connected_count -- --nocapture`
- `cargo check -p glimpse`

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add glimpse/src/dbus/bluez.rs glimpse/src/providers/bluetooth.rs
git commit -m "feat: add bluetooth trust support"
```

---

### Task 4: Implement Bluetooth Agent Prompt Bridge

**Files:**
- Create: `glimpse-panel/src/services/bluetooth/agent.rs`
- Modify: `glimpse-panel/src/services/bluetooth/protocol.rs`
- Test: `glimpse-panel/src/services/bluetooth/agent.rs`

- [ ] **Step 1: Write the failing test**

Add a unit test for prompt id allocation and reply routing:

```rust
#[test]
fn prompt_registry_completes_matching_request() {
    let mut registry = PromptRegistry::default();
    let prompt = registry.begin_request(
        "/org/bluez/hci0/dev_AA_BB".into(),
        "Headphones".into(),
        BluetoothPromptKind::RequestPin,
    );

    let reply = registry.complete(prompt.id, BluetoothPromptReply::Pin("1234".into()));

    assert_eq!(reply, Some(BluetoothPromptReply::Pin("1234".into())));
    assert!(registry.current_prompt().is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel prompt_registry_completes_matching_request -- --nocapture`
Expected: FAIL because `PromptRegistry` does not exist

- [ ] **Step 3: Write minimal prompt registry and agent surface**

Create `agent.rs` with:

```rust
#[derive(Default)]
struct PromptRegistry {
    next_id: u64,
    current: Option<BluetoothPrompt>,
}

impl PromptRegistry {
    fn begin_request(
        &mut self,
        device_path: String,
        device_label: String,
        kind: BluetoothPromptKind,
    ) -> BluetoothPrompt {
        self.next_id += 1;
        let prompt = BluetoothPrompt {
            id: BluetoothPromptId(self.next_id),
            device_path,
            device_label,
            kind,
        };
        self.current = Some(prompt.clone());
        prompt
    }

    fn current_prompt(&self) -> Option<&BluetoothPrompt> {
        self.current.as_ref()
    }

    fn complete(
        &mut self,
        id: BluetoothPromptId,
        reply: BluetoothPromptReply,
    ) -> Option<BluetoothPromptReply> {
        match &self.current {
            Some(prompt) if prompt.id == id => {
                self.current = None;
                Some(reply)
            }
            _ => None,
        }
    }
}
```

Add the future-facing BlueZ agent shell using the old `glimpsed` `Agent1` implementation as the basis, but replace `zenity` with prompt emission and awaiting reply.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p glimpse-panel prompt_registry_completes_matching_request -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/services/bluetooth/agent.rs glimpse-panel/src/services/bluetooth/protocol.rs
git commit -m "feat: add bluetooth agent prompt bridge"
```

---

### Task 5: Implement Bluetooth Service State Machine

**Files:**
- Create: `glimpse-panel/src/services/bluetooth/service.rs`
- Modify: `glimpse-panel/src/services/bluetooth/mod.rs`
- Test: `glimpse-panel/src/services/bluetooth/service.rs`

- [ ] **Step 1: Write the failing tests**

Add two unit tests:

```rust
#[test]
fn close_popover_releases_all_service_owned_discovery_claims() {
    let mut claims = DiscoveryClaims {
        initial: true,
        popover_count: 1,
    };

    assert_eq!(claims.close_popover(), 2);
    assert_eq!(claims.popover_count, 0);
    assert!(!claims.initial);
}

#[test]
fn first_popover_open_starts_discovery_but_second_does_not() {
    let mut claims = DiscoveryClaims::default();

    assert_eq!(claims.open_popover(), 1);
    assert_eq!(claims.open_popover(), 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:
- `cargo test -p glimpse-panel close_popover_releases_all_service_owned_discovery_claims -- --nocapture`
- `cargo test -p glimpse-panel first_popover_open_starts_discovery_but_second_does_not -- --nocapture`

Expected: FAIL because `DiscoveryClaims` is not defined in the service module

- [ ] **Step 3: Write minimal service core**

Create `service.rs`:

```rust
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, watch};

pub struct BluetoothServiceHandle {
    commands: mpsc::Sender<BluetoothServiceCommand>,
    state: watch::Receiver<BluetoothServiceState>,
}

impl Clone for BluetoothServiceHandle {
    fn clone(&self) -> Self {
        Self {
            commands: self.commands.clone(),
            state: self.state.clone(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct DiscoveryClaims {
    initial: bool,
    popover_count: u32,
}

impl DiscoveryClaims {
    fn open_popover(&mut self) -> u8 {
        self.popover_count += 1;
        if self.popover_count == 1 { 1 } else { 0 }
    }

    fn close_popover(&mut self) -> u8 {
        let mut stop_calls = 0;
        if self.popover_count > 0 {
            self.popover_count -= 1;
            if self.popover_count == 0 {
                stop_calls += 1;
            }
        }
        if self.initial {
            self.initial = false;
            stop_calls += 1;
        }
        stop_calls
    }
}
```

Expose a temporary constructor:

```rust
impl BluetoothServiceHandle {
    pub fn new_placeholder(system: zbus::Connection) -> Self {
        let (_tx, rx) = watch::channel(BluetoothServiceState {
            health: BluetoothServiceHealth::Starting,
            snapshot: Default::default(),
            prompt: None,
            active_action: None,
        });
        let (cmd_tx, _cmd_rx) = mpsc::channel(32);
        let _ = system;
        Self { commands: cmd_tx, state: rx }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run:
- `cargo test -p glimpse-panel close_popover_releases_all_service_owned_discovery_claims -- --nocapture`
- `cargo test -p glimpse-panel first_popover_open_starts_discovery_but_second_does_not -- --nocapture`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/services/bluetooth/service.rs glimpse-panel/src/services/bluetooth/mod.rs
git commit -m "feat: add bluetooth service state machine"
```

---

### Task 6: Implement Service Worker, Agent Startup, and Reconnect Loop

**Files:**
- Modify: `glimpse-panel/src/services/bluetooth/service.rs`
- Modify: `glimpse-panel/src/services/bluetooth/agent.rs`
- Test: `cargo check -p glimpse-panel`

- [ ] **Step 1: Write the failing compile step**

Replace `new_placeholder(system)` with a real `new(system)` that spawns a worker task and registers the agent.

```rust
let bluetooth = bluetooth::BluetoothServiceHandle::new(system.clone());
```

Run: `cargo check -p glimpse-panel`
Expected: FAIL because `BluetoothServiceHandle::new` and worker code do not exist

- [ ] **Step 2: Write minimal worker loop**

Add to `service.rs`:

```rust
pub fn new(system: zbus::Connection) -> Self {
    let (state_tx, state_rx) = watch::channel(BluetoothServiceState {
        health: BluetoothServiceHealth::Starting,
        snapshot: Default::default(),
        prompt: None,
        active_action: None,
    });
    let (cmd_tx, cmd_rx) = mpsc::channel(64);

    relm4::spawn({
        let system = system.clone();
        async move {
            run_bluetooth_service(system, state_tx, cmd_rx).await;
        }
    });

    Self { commands: cmd_tx, state: state_rx }
}
```

Worker skeleton:

```rust
async fn run_bluetooth_service(
    system: zbus::Connection,
    state_tx: watch::Sender<BluetoothServiceState>,
    mut cmd_rx: mpsc::Receiver<BluetoothServiceCommand>,
) {
    let provider = BluetoothProvider::new(system.clone());
    let mut attempt = 0u32;

    loop {
        attempt += 1;
        let _ = state_tx.send_modify(|state| {
            state.health = if attempt == 1 {
                BluetoothServiceHealth::Starting
            } else {
                BluetoothServiceHealth::Reconnecting { attempt }
            };
        });

        match run_connected(provider.clone(), state_tx.clone(), &mut cmd_rx).await {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "bluetooth service: worker failed");
                let _ = state_tx.send_modify(|state| {
                    state.health = BluetoothServiceHealth::Degraded {
                        message: error.to_string(),
                    };
                });
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }
}
```

- [ ] **Step 3: Hook in agent startup**

Inside `run_connected(...)`, register the BlueZ agent before entering the main select loop:

```rust
let agent = BluetoothAgentHandle::register(system.clone(), state_tx.clone()).await?;
```

Then create listener stream + initial scan and handle commands/signals in one loop.

- [ ] **Step 4: Run verification**

Run:
- `cargo check -p glimpse`
- `cargo check -p glimpse-panel`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/services/bluetooth/service.rs glimpse-panel/src/services/bluetooth/agent.rs glimpse-panel/src/services/mod.rs glimpse-panel/src/app.rs
git commit -m "feat: start bluetooth service and agent at app level"
```

---

### Task 7: Refactor Bluetooth Applet to Use the Service

**Files:**
- Modify: `glimpse-panel/src/applets/bluetooth/applet.rs`
- Modify: `glimpse-panel/src/applets/mod.rs`
- Test: `cargo test -p glimpse-panel subtitle_prefers_activity_then_discovery_then_connection_state -- --nocapture`

- [ ] **Step 1: Write the failing compile change**

Change the applet init from:

```rust
pub struct BluetoothInit {
    pub config: BluetoothConfig,
    pub conn: zbus::Connection,
}
```

to:

```rust
pub struct BluetoothInit {
    pub config: BluetoothConfig,
    pub service: BluetoothServiceHandle,
}
```

Run: `cargo check -p glimpse-panel`
Expected: FAIL because applet factory and init logic still expect `conn`

- [ ] **Step 2: Replace provider ownership with service subscription**

Inside `init()`, subscribe to service state and forward updates into applet messages:

```rust
let service = init.service.clone();

sender.command(move |out, shutdown| {
    shutdown
        .register(async move {
            let mut state_rx = service.subscribe();
            loop {
                if state_rx.changed().await.is_err() {
                    break;
                }
                let state = state_rx.borrow().clone();
                let _ = out.send(BluetoothMsg::StatusUpdate {
                    powered: state.snapshot.status.powered,
                    discovering: state.snapshot.status.discovering,
                    connected_count: state.snapshot.status.connected_count,
                });
                let _ = out.send(BluetoothMsg::DevicesUpdate(popover_devices(state.snapshot)));
            }
        })
        .drop_on_shutdown()
});
```

- [ ] **Step 3: Convert applet commands to service commands**

Replace queue/provider actions with:

```rust
service.send(BluetoothServiceCommand::StartDiscovery).await?;
service.send(BluetoothServiceCommand::Pair { address }).await?;
service.send(BluetoothServiceCommand::Trust { address, trusted: true }).await?;
```

Remove:
- applet-owned provider construction
- applet-local listener spawning
- applet-local action queue tied to provider methods

- [ ] **Step 4: Run verification**

Run:
- `cargo test -p glimpse-panel subtitle_prefers_activity_then_discovery_then_connection_state -- --nocapture`
- `cargo check -p glimpse-panel`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/applets/bluetooth/applet.rs glimpse-panel/src/applets/mod.rs glimpse-panel/src/panels/component.rs glimpse-panel/src/app.rs
git commit -m "refactor: move bluetooth applet to app-level service"
```

---

### Task 8: Add GTK Pairing Dialogs Backed by Service Prompt State

**Files:**
- Create: `glimpse-panel/src/services/bluetooth/dialogs.rs`
- Modify: `glimpse-panel/src/services/bluetooth/service.rs`
- Modify: `glimpse-panel/src/applets/bluetooth/applet.rs`
- Test: `cargo check -p glimpse-panel`

- [ ] **Step 1: Write the failing prompt UI test**

Add a pure helper test in `dialogs.rs`:

```rust
#[test]
fn confirm_prompt_uses_pair_action_label() {
    let view = prompt_view_model(&BluetoothPrompt {
        id: BluetoothPromptId(1),
        device_path: "/org/bluez/hci0/dev_AA_BB".into(),
        device_label: "MX Keys Mini".into(),
        kind: BluetoothPromptKind::Confirm { passkey: 482931 },
    });

    assert_eq!(view.title, "Bluetooth Pairing");
    assert_eq!(view.accept_label, "Pair");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel confirm_prompt_uses_pair_action_label -- --nocapture`
Expected: FAIL because `prompt_view_model` does not exist

- [ ] **Step 3: Write minimal dialog model**

Create:

```rust
struct PromptViewModel {
    title: String,
    message: String,
    accept_label: String,
}

fn prompt_view_model(prompt: &BluetoothPrompt) -> PromptViewModel {
    match &prompt.kind {
        BluetoothPromptKind::Confirm { passkey } => PromptViewModel {
            title: "Bluetooth Pairing".into(),
            message: format!("Pair with {}?\nCode: {:06}", prompt.device_label, passkey),
            accept_label: "Pair".into(),
        },
        BluetoothPromptKind::RequestPin => PromptViewModel {
            title: "Bluetooth Pairing".into(),
            message: format!("Enter PIN for {}", prompt.device_label),
            accept_label: "Submit".into(),
        },
        _ => PromptViewModel {
            title: "Bluetooth Pairing".into(),
            message: prompt.device_label.clone(),
            accept_label: "Submit".into(),
        },
    }
}
```

Then add a simple GTK dialog presenter bound to service prompt state and sending `PromptReply`.

- [ ] **Step 4: Run verification**

Run:
- `cargo test -p glimpse-panel confirm_prompt_uses_pair_action_label -- --nocapture`
- `cargo check -p glimpse-panel`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/services/bluetooth/dialogs.rs glimpse-panel/src/services/bluetooth/service.rs glimpse-panel/src/applets/bluetooth/applet.rs
git commit -m "feat: add bluetooth pairing dialogs"
```

---

### Task 9: Finish Bluetooth Command Coverage and Cleanup

**Files:**
- Modify: `glimpse-panel/src/services/bluetooth/service.rs`
- Modify: `glimpse-panel/src/applets/bluetooth/applet.rs`
- Modify: `glimpse-panel/src/applets/bluetooth/popover.rs`
- Test: `cargo test -p glimpse-panel`
- Test: `cargo test -p glimpse`

- [ ] **Step 1: Write the failing service behavior test**

Add a reducer/state test that verifies pairing success schedules a trust command and that forget clears active action state.

```rust
#[test]
fn forget_clears_matching_active_action() {
    let mut state = BluetoothServiceState {
        health: BluetoothServiceHealth::Ready,
        snapshot: BluetoothSnapshot::default(),
        prompt: None,
        active_action: Some(BluetoothActiveAction::Forget {
            address: "AA:BB:CC:DD:EE:FF".into(),
        }),
    };

    clear_active_action_for(&mut state, "AA:BB:CC:DD:EE:FF");

    assert!(state.active_action.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel forget_clears_matching_active_action -- --nocapture`
Expected: FAIL because `clear_active_action_for` does not exist

- [ ] **Step 3: Implement the missing glue**

Add helper logic:

```rust
fn clear_active_action_for(state: &mut BluetoothServiceState, address: &str) {
    let clear = matches!(
        state.active_action.as_ref(),
        Some(BluetoothActiveAction::Connect { address: current })
            | Some(BluetoothActiveAction::Disconnect { address: current })
            | Some(BluetoothActiveAction::Pair { address: current })
            | Some(BluetoothActiveAction::Trust { address: current, .. })
            | Some(BluetoothActiveAction::Forget { address: current })
            if current == address
    );

    if clear {
        state.active_action = None;
    }
}
```

Then ensure service command handling covers:
- `Connect`
- `Disconnect`
- `Pair`
- `Trust`
- `Forget`
- `SetPowered`
- `StartDiscovery`
- `StopDiscovery`

and that applet status text reflects `active_action` / prompt state cleanly.

- [ ] **Step 4: Run full verification**

Run:
- `cargo test -p glimpse -- --nocapture`
- `cargo test -p glimpse-panel -- --nocapture`
- `cargo check -p glimpse`
- `cargo check -p glimpse-panel`

Expected: PASS, with only existing unrelated warnings

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/services/bluetooth/service.rs glimpse-panel/src/applets/bluetooth/applet.rs glimpse-panel/src/applets/bluetooth/popover.rs glimpse/src/providers/bluetooth.rs
git commit -m "feat: complete bluetooth service command flow"
```

---

## Self-Review

### Spec coverage
- App-level `Services` container: Task 1
- Bluetooth service and typed handle: Tasks 1, 2, 5, 6
- Provider remains method-based and gains `trust`: Task 3
- BlueZ `Agent1` port: Task 4
- GTK dialogs instead of `zenity`: Task 8
- Applet consumes service state/commands only: Task 7
- Discovery ownership for multiple applets: Tasks 5, 6, 7
- Crash/reconnect handling: Task 6
- Full command coverage (`pair`, `trust`, `forget`, `connect`, `disconnect`, discovery): Tasks 3, 6, 9

### Placeholder scan
- No `TODO` / `TBD`
- All tasks specify exact files
- All code steps include concrete snippets
- All verification steps specify exact commands

### Type consistency
- Service handle type: `BluetoothServiceHandle`
- state type: `BluetoothServiceState`
- commands type: `BluetoothServiceCommand`
- prompt types: `BluetoothPrompt`, `BluetoothPromptId`, `BluetoothPromptReply`
- provider method addition: `trust(address, trusted)`

