use std::{collections::HashMap, path::PathBuf, time::Duration};

use anyhow::{Context, bail};
use futures_util::{StreamExt, future};
use serde::Serialize;
use tokio::{
    sync::{mpsc, watch},
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use zbus::{
    MatchRule, MessageStream,
    message::Type,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue},
};

use crate::{
    dbus::udisks2::{DriveProxy, FilesystemProxy},
    services::framework::{Control, ServiceCommand, ServiceHandle},
};

const UDISKS_SERVICE: &str = "org.freedesktop.UDisks2";
const UDISKS_ROOT: &str = "/org/freedesktop/UDisks2";
const BLOCK_IFACE: &str = "org.freedesktop.UDisks2.Block";
const DRIVE_IFACE: &str = "org.freedesktop.UDisks2.Drive";
const FILESYSTEM_IFACE: &str = "org.freedesktop.UDisks2.Filesystem";
const LISTENER_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct State {
    pub available: bool,
    pub devices: Vec<StorageDevice>,
    pub active_action: Option<StorageActiveAction>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct StorageDevice {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub kind: StorageKind,
    pub size_bytes: Option<u64>,
    pub mounted_at: Option<PathBuf>,
    pub filesystem: Option<String>,
    pub removable: bool,
    pub ejectable: bool,
    pub can_mount: bool,
    pub can_unmount: bool,
    pub can_eject: bool,
    pub can_power_off: bool,
    pub busy: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
pub enum StorageKind {
    #[default]
    Drive,
    Optical,
    Card,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum StorageActiveAction {
    Mount { id: String },
    Unmount { id: String },
    Eject { id: String },
    PowerOff { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh,
    Mount { id: String },
    Unmount { id: String },
    Eject { id: String },
    PowerOff { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageChangeReason {
    InterfacesAdded,
    InterfacesRemoved,
    PropertiesChanged,
    Mixed,
}

pub type StorageHandle = ServiceHandle<State, Command>;

pub struct StorageService {
    client: UDisks2Client,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

#[derive(Clone)]
pub struct UDisks2Client {
    conn: zbus::Connection,
}

impl StorageService {
    pub fn new(conn: zbus::Connection) -> (Self, StorageHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(16);

        (
            Self {
                client: UDisks2Client::new(conn),
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        if let Err(error) = self.run_inner(cancel).await {
            tracing::warn!(error = %error, "storage service failed");
            self.update_state(|state| {
                state.available = false;
                state.error = Some(error.to_string());
            });
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<()> {
        tracing::debug!("storage service started");
        self.refresh_snapshot()
            .await
            .context("failed to load initial storage snapshot")?;

        let (event_tx, mut event_rx) = mpsc::channel(32);
        let (action_tx, mut action_rx) = mpsc::channel::<anyhow::Result<()>>(16);
        let listener_cancel = CancellationToken::new();
        let listener =
            spawn_storage_listener(self.client.clone(), event_tx, listener_cancel.clone());

        let outcome = loop {
            tokio::select! {
                _ = cancel.cancelled() => break Ok(()),
                event = event_rx.recv() => match event {
                    Some(reason) => {
                        tracing::debug!(reason = ?reason, "storage: refreshing service state");
                        if let Err(error) = self.refresh_snapshot().await {
                            tracing::warn!(error = %error, "storage: refresh failed after change event");
                            self.set_degraded("Storage data is stale");
                        }
                    }
                    None => break Err(anyhow::anyhow!("storage event listener stopped")),
                },
                result = action_rx.recv() => match result {
                    Some(result) => {
                        self.update_state(|state| state.active_action = None);
                        match result {
                            Ok(()) => {
                                if let Err(error) = self.refresh_snapshot().await {
                                    tracing::warn!(error = %error, "storage: refresh failed after command");
                                    self.set_degraded("Storage data is stale");
                                }
                            }
                            Err(error) => {
                                tracing::warn!(error = %error, "storage command failed");
                                if let Err(refresh_error) = self.refresh_snapshot().await {
                                    tracing::warn!(error = %refresh_error, "storage: refresh failed after failed command");
                                    self.set_degraded("Storage data is stale");
                                }
                                self.update_state(|state| {
                                    state.error = Some(error.to_string());
                                });
                            }
                        }
                    }
                    None => break Err(anyhow::anyhow!("storage command result channel closed")),
                },
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => {
                        if self.execute_command(command, action_tx.clone()).await {
                            if let Err(error) = self.refresh_snapshot().await {
                                tracing::warn!(error = %error, "storage: refresh failed after command");
                                self.set_degraded("Storage data is stale");
                            }
                        }
                    }
                    Some(ServiceCommand::Control(control)) => match control {
                        Control::Start(_) | Control::Reconfigure(_) => {
                            if let Err(error) = self.refresh_snapshot().await {
                                tracing::warn!(error = %error, "storage: refresh failed after control");
                                self.set_degraded("Storage data is stale");
                            }
                        }
                        Control::Shutdown => break Ok(()),
                    },
                    None => break Ok(()),
                },
            }
        };

        listener_cancel.cancel();
        let _ = listener.await;

        outcome
    }

    async fn refresh_snapshot(&self) -> anyhow::Result<()> {
        let devices = self.client.scan().await?;
        self.update_state(|state| {
            state.available = true;
            state.error = None;
            state.devices = apply_active_action(devices, state.active_action.as_ref());
        });
        Ok(())
    }

    async fn execute_command(
        &mut self,
        command: Command,
        action_tx: mpsc::Sender<anyhow::Result<()>>,
    ) -> bool {
        match command {
            Command::Refresh => true,
            Command::Mount { id } => {
                self.spawn_action(StorageActiveAction::Mount { id }, action_tx);
                false
            }
            Command::Unmount { id } => {
                self.spawn_action(StorageActiveAction::Unmount { id }, action_tx);
                false
            }
            Command::Eject { id } => {
                self.spawn_action(StorageActiveAction::Eject { id }, action_tx);
                false
            }
            Command::PowerOff { id } => {
                self.spawn_action(StorageActiveAction::PowerOff { id }, action_tx);
                false
            }
        }
    }

    fn spawn_action(
        &mut self,
        action: StorageActiveAction,
        action_tx: mpsc::Sender<anyhow::Result<()>>,
    ) {
        if self.state_tx.borrow().active_action.is_some() {
            tracing::warn!("storage: command ignored while another action is active");
            return;
        }

        self.update_state(|state| {
            state.active_action = Some(action.clone());
            state.error = None;
            state.devices =
                apply_active_action(state.devices.clone(), state.active_action.as_ref());
        });
        spawn_storage_action(self.client.clone(), action, action_tx);
    }

    fn set_degraded(&self, message: &str) {
        self.update_state(|state| {
            state.available = false;
            state.error = Some(message.into());
        });
    }

    fn update_state(&self, update: impl FnOnce(&mut State)) {
        let mut next = self.state_tx.borrow().clone();
        update(&mut next);
        if should_emit_state(&self.state_tx.borrow(), &next) {
            self.change_state(next);
        }
    }

    fn change_state(&self, state: State) {
        if self.state_tx.send(state).is_err() {
            tracing::debug!("storage: state receiver dropped");
        }
    }
}

impl UDisks2Client {
    pub fn new(conn: zbus::Connection) -> Self {
        Self { conn }
    }

    pub async fn scan(&self) -> anyhow::Result<Vec<StorageDevice>> {
        let om = self.object_manager().await?;
        let objects = om
            .get_managed_objects()
            .await
            .context("failed to read UDisks2 managed objects")?;

        let mut devices = Vec::new();
        for (path, interfaces) in &objects {
            let Some(block) = interfaces.get(BLOCK_IFACE) else {
                continue;
            };
            let Some(filesystem) = interfaces.get(FILESYSTEM_IFACE) else {
                continue;
            };

            let drive_path = object_path_property(block, "Drive").unwrap_or_default();
            let drive = objects
                .get(&drive_path)
                .and_then(|interfaces| interfaces.get(DRIVE_IFACE));
            let Some(device) = storage_device_from_properties(
                &path.to_string(),
                block,
                filesystem,
                drive,
                &drive_path.to_string(),
            ) else {
                continue;
            };
            devices.push(device);
        }

        devices.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        tracing::debug!(devices = devices.len(), "storage: scan complete");
        Ok(devices)
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<StorageChangeReason>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        tracing::info!("storage: listener started");

        let om = self.object_manager().await?;
        let mut added = om.receive_interfaces_added().await?;
        let mut removed = om.receive_interfaces_removed().await?;
        let mut properties = self.properties_changed_stream().await?;
        let mut pending_reason: Option<StorageChangeReason> = None;
        let mut debounce_deadline: Option<Instant> = None;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("storage: listener stopping");
                    break;
                }
                signal = added.next() => match signal {
                    Some(_) => {
                        pending_reason = Some(merge_change_reason(pending_reason, StorageChangeReason::InterfacesAdded));
                        debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                    }
                    None => break,
                },
                signal = removed.next() => match signal {
                    Some(_) => {
                        pending_reason = Some(merge_change_reason(pending_reason, StorageChangeReason::InterfacesRemoved));
                        debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                    }
                    None => break,
                },
                signal = properties.next() => match signal {
                    Some(Ok(message)) => {
                        if !is_udisks_properties_changed(&message) {
                            continue;
                        }
                        pending_reason = Some(merge_change_reason(pending_reason, StorageChangeReason::PropertiesChanged));
                        debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                    }
                    Some(Err(error)) => {
                        tracing::warn!(error = %error, "storage: properties stream error");
                    }
                    None => break,
                },
                _ = async {
                    match debounce_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => future::pending::<()>().await,
                    }
                }, if debounce_deadline.is_some() => {
                    let reason = pending_reason.take().unwrap_or(StorageChangeReason::Mixed);
                    debounce_deadline = None;
                    if events.send(reason).await.is_err() {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn mount(&self, id: &str) -> anyhow::Result<()> {
        tracing::info!(id, "storage: mount requested");
        let proxy = self.filesystem_proxy(id).await?;
        proxy.mount(HashMap::new()).await?;
        tracing::info!(id, "storage: mount succeeded");
        Ok(())
    }

    pub async fn unmount(&self, id: &str) -> anyhow::Result<()> {
        tracing::info!(id, "storage: unmount requested");
        let proxy = self.filesystem_proxy(id).await?;
        proxy.unmount(HashMap::new()).await?;
        tracing::info!(id, "storage: unmount succeeded");
        Ok(())
    }

    pub async fn eject(&self, id: &str) -> anyhow::Result<()> {
        let drive_path = self.drive_path_for_block(id).await?;
        tracing::info!(id, drive = %drive_path, "storage: eject requested");
        let proxy = self.drive_proxy(&drive_path).await?;
        proxy.eject(HashMap::new()).await?;
        tracing::info!(id, drive = %drive_path, "storage: eject succeeded");
        Ok(())
    }

    pub async fn power_off(&self, id: &str) -> anyhow::Result<()> {
        let drive_path = self.drive_path_for_block(id).await?;
        tracing::info!(id, drive = %drive_path, "storage: power off requested");
        let proxy = self.drive_proxy(&drive_path).await?;
        proxy.power_off(HashMap::new()).await?;
        tracing::info!(id, drive = %drive_path, "storage: power off succeeded");
        Ok(())
    }

    async fn drive_path_for_block(&self, id: &str) -> anyhow::Result<String> {
        let om = self.object_manager().await?;
        let objects = om.get_managed_objects().await?;
        let path = ObjectPath::try_from(id).context("invalid storage object path")?;
        let Some(interfaces) = objects.get(&path) else {
            bail!("unknown storage device: {id}");
        };
        let Some(block) = interfaces.get(BLOCK_IFACE) else {
            bail!("storage device has no block interface: {id}");
        };
        let drive_path = object_path_property(block, "Drive");
        if drive_path.as_ref().is_none_or(|path| path.as_str() == "/") {
            bail!("storage device has no drive: {id}");
        }
        Ok(drive_path.expect("checked above").to_string())
    }

    async fn object_manager(&self) -> anyhow::Result<zbus::fdo::ObjectManagerProxy<'_>> {
        zbus::fdo::ObjectManagerProxy::builder(&self.conn)
            .destination(UDISKS_SERVICE)?
            .path(UDISKS_ROOT)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn filesystem_proxy<'a>(&self, path: &'a str) -> anyhow::Result<FilesystemProxy<'a>> {
        FilesystemProxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn drive_proxy<'a>(&self, path: &'a str) -> anyhow::Result<DriveProxy<'a>> {
        DriveProxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn properties_changed_stream(&self) -> anyhow::Result<MessageStream> {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(UDISKS_SERVICE)?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .build();

        MessageStream::for_match_rule(rule, &self.conn, None)
            .await
            .map_err(Into::into)
    }
}

fn spawn_storage_listener(
    client: UDisks2Client,
    events: mpsc::Sender<StorageChangeReason>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move { client.listen(events, cancel).await })
}

fn spawn_storage_action(
    client: UDisks2Client,
    action: StorageActiveAction,
    done: mpsc::Sender<anyhow::Result<()>>,
) {
    tokio::spawn(async move {
        let result = execute_storage_action(client, action).await;
        let _ = done.send(result).await;
    });
}

async fn execute_storage_action(
    client: UDisks2Client,
    action: StorageActiveAction,
) -> anyhow::Result<()> {
    match action {
        StorageActiveAction::Mount { id } => client.mount(&id).await,
        StorageActiveAction::Unmount { id } => client.unmount(&id).await,
        StorageActiveAction::Eject { id } => client.eject(&id).await,
        StorageActiveAction::PowerOff { id } => client.power_off(&id).await,
    }
}

fn storage_device_from_properties(
    path: &str,
    block: &HashMap<String, OwnedValue>,
    filesystem: &HashMap<String, OwnedValue>,
    drive: Option<&HashMap<String, OwnedValue>>,
    drive_path: &str,
) -> Option<StorageDevice> {
    if bool_property(block, "HintIgnore") || bool_property(block, "HintSystem") {
        return None;
    }

    let removable = drive.is_some_and(drive_is_removable);
    if !removable {
        return None;
    }

    let mount_points = mount_points_property(filesystem, "MountPoints");
    let mounted_at = mount_points.into_iter().next();
    let ejectable = drive.is_some_and(drive_is_ejectable);
    let can_power_off = drive.is_some_and(|drive| bool_property(drive, "CanPowerOff"));
    let icon = first_non_empty(&[
        string_property(block, "HintSymbolicIconName"),
        drive
            .and_then(|drive| string_property(drive, "Media"))
            .map(media_icon_name),
        Some("drive-removable-media-symbolic".into()),
    ]);

    Some(StorageDevice {
        id: path.into(),
        name: storage_device_name(path, block, drive),
        icon,
        kind: storage_kind(drive),
        size_bytes: u64_property(block, "Size"),
        mounted_at: mounted_at.clone(),
        filesystem: string_property(block, "IdType"),
        removable,
        ejectable,
        can_mount: mounted_at.is_none(),
        can_unmount: mounted_at.is_some(),
        can_eject: ejectable,
        can_power_off,
        busy: false,
        error: None,
    })
    .inspect(|device| {
        tracing::debug!(
            path,
            drive = drive_path,
            name = %device.name,
            mounted = device.mounted_at.is_some(),
            "storage: discovered removable filesystem"
        );
    })
}

fn storage_device_name(
    path: &str,
    block: &HashMap<String, OwnedValue>,
    drive: Option<&HashMap<String, OwnedValue>>,
) -> String {
    first_non_empty(&[
        string_property(block, "IdLabel"),
        string_property(block, "HintName"),
        drive.and_then(drive_display_name),
        string_property(block, "PreferredDevice")
            .or_else(|| string_property(block, "Device"))
            .and_then(|device| {
                PathBuf::from(device)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            }),
        Some(path.rsplit('/').next().unwrap_or(path).into()),
    ])
}

fn drive_display_name(drive: &HashMap<String, OwnedValue>) -> Option<String> {
    let vendor = string_property(drive, "Vendor").unwrap_or_default();
    let model = string_property(drive, "Model").unwrap_or_default();
    let name = format!("{vendor} {model}").trim().to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn drive_is_removable(drive: &HashMap<String, OwnedValue>) -> bool {
    bool_property(drive, "Removable")
        || bool_property(drive, "MediaRemovable")
        || string_property(drive, "ConnectionBus").is_some_and(|bus| bus == "usb" || bus == "sdio")
}

fn drive_is_ejectable(drive: &HashMap<String, OwnedValue>) -> bool {
    bool_property(drive, "Ejectable") || bool_property(drive, "MediaRemovable")
}

fn storage_kind(drive: Option<&HashMap<String, OwnedValue>>) -> StorageKind {
    let Some(drive) = drive else {
        return StorageKind::Drive;
    };
    let media = string_property(drive, "Media").unwrap_or_default();
    if bool_property(drive, "Optical") || media.contains("optical") || media.contains("cd") {
        StorageKind::Optical
    } else if media.contains("flash_sd") || media.contains("flash_mmc") {
        StorageKind::Card
    } else {
        StorageKind::Drive
    }
}

fn media_icon_name(media: String) -> String {
    if media.contains("optical") || media.contains("cd") {
        "media-optical-symbolic".into()
    } else if media.contains("flash_sd") || media.contains("flash_mmc") {
        "media-flash-sd-mmc-symbolic".into()
    } else {
        "drive-removable-media-symbolic".into()
    }
}

fn apply_active_action(
    mut devices: Vec<StorageDevice>,
    action: Option<&StorageActiveAction>,
) -> Vec<StorageDevice> {
    let Some(action) = action else {
        return devices;
    };
    let id = active_action_id(action);
    for device in &mut devices {
        device.busy = device.id == id;
    }
    devices
}

fn active_action_id(action: &StorageActiveAction) -> &str {
    match action {
        StorageActiveAction::Mount { id }
        | StorageActiveAction::Unmount { id }
        | StorageActiveAction::Eject { id }
        | StorageActiveAction::PowerOff { id } => id,
    }
}

fn string_property(props: &HashMap<String, OwnedValue>, name: &str) -> Option<String> {
    props
        .get(name)
        .and_then(|value| String::try_from(value.clone()).ok())
        .filter(|value| !value.is_empty())
}

fn bool_property(props: &HashMap<String, OwnedValue>, name: &str) -> bool {
    props
        .get(name)
        .and_then(|value| bool::try_from(value.clone()).ok())
        .unwrap_or(false)
}

fn u64_property(props: &HashMap<String, OwnedValue>, name: &str) -> Option<u64> {
    props
        .get(name)
        .and_then(|value| u64::try_from(value.clone()).ok())
        .filter(|value| *value > 0)
}

fn object_path_property(
    props: &HashMap<String, OwnedValue>,
    name: &str,
) -> Option<OwnedObjectPath> {
    props
        .get(name)
        .and_then(|value| OwnedObjectPath::try_from(value.clone()).ok())
}

fn mount_points_property(props: &HashMap<String, OwnedValue>, name: &str) -> Vec<PathBuf> {
    let Some(value) = props.get(name) else {
        return Vec::new();
    };

    Vec::<Vec<u8>>::try_from(value.clone())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|bytes| {
            let trimmed = bytes
                .strip_suffix(&[0])
                .map(ToOwned::to_owned)
                .unwrap_or(bytes);
            if trimmed.is_empty() {
                return None;
            }
            Some(PathBuf::from(String::from_utf8_lossy(&trimmed).to_string()))
        })
        .collect()
}

fn first_non_empty(values: &[Option<String>]) -> String {
    values
        .iter()
        .flatten()
        .find(|value| !value.is_empty())
        .cloned()
        .unwrap_or_default()
}

fn should_emit_state(current: &State, next: &State) -> bool {
    current != next
}

fn merge_change_reason(
    current: Option<StorageChangeReason>,
    next: StorageChangeReason,
) -> StorageChangeReason {
    match current {
        None => next,
        Some(current) if current == next => current,
        Some(_) => StorageChangeReason::Mixed,
    }
}

fn is_udisks_properties_changed(message: &zbus::message::Message) -> bool {
    let header = message.header();
    let Some(path) = header.path() else {
        return false;
    };
    if !path.as_str().starts_with(UDISKS_ROOT) {
        return false;
    }

    match message
        .body()
        .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
    {
        Ok((interface, changed, invalidated)) => udisks_properties_are_relevant(
            &interface,
            changed.keys().map(String::as_str),
            invalidated.iter().map(String::as_str),
        ),
        Err(error) => {
            tracing::debug!(%error, "storage: failed to inspect properties changed body");
            true
        }
    }
}

fn udisks_properties_are_relevant<'a>(
    interface: &str,
    changed: impl Iterator<Item = &'a str>,
    invalidated: impl Iterator<Item = &'a str>,
) -> bool {
    changed
        .chain(invalidated)
        .any(|property| udisks_property_is_relevant(interface, property))
}

fn udisks_property_is_relevant(interface: &str, property: &str) -> bool {
    match interface {
        BLOCK_IFACE => matches!(
            property,
            "Drive"
                | "Device"
                | "PreferredDevice"
                | "IdLabel"
                | "IdType"
                | "Size"
                | "HintIgnore"
                | "HintSystem"
                | "HintAuto"
                | "HintName"
                | "HintSymbolicIconName"
        ),
        FILESYSTEM_IFACE => property == "MountPoints",
        DRIVE_IFACE => matches!(
            property,
            "Vendor"
                | "Model"
                | "Media"
                | "MediaRemovable"
                | "Removable"
                | "Ejectable"
                | "CanPowerOff"
                | "Optical"
                | "ConnectionBus"
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn owned_value(value: Value<'_>) -> OwnedValue {
        OwnedValue::try_from(value).expect("test value should be representable as an owned value")
    }

    #[test]
    fn filters_internal_system_filesystems() {
        let block = HashMap::from([
            ("HintSystem".into(), OwnedValue::from(true)),
            ("HintAuto".into(), OwnedValue::from(true)),
        ]);
        let fs = HashMap::new();

        assert!(storage_device_from_properties("/dev", &block, &fs, None, "/").is_none());
    }

    #[test]
    fn maps_removable_mounted_filesystem_to_device() {
        let block = HashMap::from([
            ("IdLabel".into(), owned_value(Value::from("Work USB"))),
            ("IdType".into(), owned_value(Value::from("vfat"))),
            ("HintAuto".into(), OwnedValue::from(true)),
            ("Size".into(), OwnedValue::from(16_u64 * 1024 * 1024 * 1024)),
        ]);
        let fs = HashMap::from([(
            "MountPoints".into(),
            owned_value(Value::from(vec![b"/run/media/alex/Work USB\0".to_vec()])),
        )]);
        let drive = HashMap::from([
            ("Removable".into(), OwnedValue::from(true)),
            ("ConnectionBus".into(), owned_value(Value::from("usb"))),
        ]);

        let device = storage_device_from_properties(
            "/org/freedesktop/UDisks2/block_devices/sdb1",
            &block,
            &fs,
            Some(&drive),
            "/org/freedesktop/UDisks2/drives/usb",
        )
        .expect("removable filesystem should be visible");

        assert_eq!(device.name, "Work USB");
        assert_eq!(device.filesystem.as_deref(), Some("vfat"));
        assert_eq!(
            device.mounted_at,
            Some(PathBuf::from("/run/media/alex/Work USB"))
        );
        assert!(!device.can_mount);
        assert!(device.can_unmount);
    }

    #[test]
    fn automount_hint_without_removable_drive_is_hidden() {
        let block = HashMap::from([
            ("IdLabel".into(), owned_value(Value::from("Internal Data"))),
            ("HintAuto".into(), OwnedValue::from(true)),
        ]);
        let fs = HashMap::new();

        assert!(
            storage_device_from_properties(
                "/org/freedesktop/UDisks2/block_devices/nvme0n1p3",
                &block,
                &fs,
                None,
                "/",
            )
            .is_none()
        );
    }

    #[test]
    fn active_action_marks_only_matching_device_busy() {
        let devices = vec![
            StorageDevice {
                id: "a".into(),
                name: "A".into(),
                icon: String::new(),
                kind: StorageKind::Drive,
                size_bytes: None,
                mounted_at: None,
                filesystem: None,
                removable: true,
                ejectable: false,
                can_mount: true,
                can_unmount: false,
                can_eject: false,
                can_power_off: false,
                busy: false,
                error: None,
            },
            StorageDevice {
                id: "b".into(),
                name: "B".into(),
                icon: String::new(),
                kind: StorageKind::Drive,
                size_bytes: None,
                mounted_at: None,
                filesystem: None,
                removable: true,
                ejectable: false,
                can_mount: true,
                can_unmount: false,
                can_eject: false,
                can_power_off: false,
                busy: false,
                error: None,
            },
        ];

        let devices = apply_active_action(
            devices,
            Some(&StorageActiveAction::Mount { id: "b".into() }),
        );

        assert!(!devices[0].busy);
        assert!(devices[1].busy);
    }

    #[test]
    fn property_filter_keeps_only_storage_relevant_changes() {
        assert!(udisks_property_is_relevant(FILESYSTEM_IFACE, "MountPoints"));
        assert!(udisks_property_is_relevant(BLOCK_IFACE, "IdLabel"));
        assert!(udisks_property_is_relevant(DRIVE_IFACE, "CanPowerOff"));
        assert!(!udisks_property_is_relevant(DRIVE_IFACE, "SortKey"));
        assert!(!udisks_property_is_relevant(
            "org.freedesktop.DBus.Introspectable",
            "Anything"
        ));
    }

    #[test]
    fn should_emit_state_only_for_real_changes() {
        let current = State::default();
        assert!(!should_emit_state(&current, &current));
        let mut next = current.clone();
        next.available = true;
        assert!(should_emit_state(&current, &next));
    }
}
