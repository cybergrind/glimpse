use std::path::PathBuf;

use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use zbus::zvariant::OwnedObjectPath;

use crate::{
    dbus::upower::{UPowerDeviceProxy, UPowerProxy},
    services::framework::{ServiceCommand, ServiceHandle},
};

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
#[repr(u32)]
pub enum DeviceType {
    #[default]
    Unknown = 0,
    LinePower = 1,
    Battery = 2,
    Ups = 3,
    Monitor = 4,
    Mouse = 5,
    Keyboard = 6,
    Pda = 7,
    Phone = 8,
}

impl From<u32> for DeviceType {
    fn from(t: u32) -> Self {
        match t {
            1 => Self::LinePower,
            2 => Self::Battery,
            3 => Self::Ups,
            4 => Self::Monitor,
            5 => Self::Mouse,
            6 => Self::Keyboard,
            7 => Self::Pda,
            8 => Self::Phone,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum BatteryState {
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
    #[default]
    Unknown,
}

impl From<u32> for BatteryState {
    fn from(s: u32) -> Self {
        match s {
            1 => Self::Charging,
            2 => Self::Discharging,
            3 => Self::Empty,
            4 => Self::FullyCharged,
            5 => Self::PendingCharge,
            6 => Self::PendingDischarge,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct BatteryStatus {
    pub present: bool,
    pub device_type: DeviceType,
    pub model: String,
    pub percentage: u8,
    pub state: BatteryState,
    pub icon_name: String,
    pub on_battery: bool,
    pub time_to_empty: i64,
    pub time_to_full: i64,
    pub energy_rate: f64,
    pub capacity: f64,
    pub charge_threshold: u32,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct BatteryDevice {
    pub path: String,
    pub device_type: DeviceType,
    pub model: String,
    pub percentage: f64,
    pub state: BatteryState,
    pub icon_name: String,
}

#[derive(Debug, Clone)]
pub enum Command {
    Refresh,
}

#[derive(Debug, Clone)]
pub struct State {
    pub status: BatteryStatus,
    pub devices: Vec<BatteryDevice>,
    pub threshold_path: Option<PathBuf>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            status: BatteryStatus::default(),
            devices: vec![],
            threshold_path: None,
        }
    }
}

pub type BatteryHandle = ServiceHandle<State, Command>;

pub struct BatteryService {
    conn: zbus::Connection,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

impl BatteryService {
    pub fn new(conn: zbus::Connection) -> (Self, ServiceHandle<State, Command>) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(4);

        (
            Self {
                conn,
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    fn change_state(&self, state: State) {
        if let Err(err) = self.state_tx.send(state) {
            tracing::error!("failed to send new state: {:?}", err);
        }
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        if let Err(error) = self.run_inner(cancel).await {
            tracing::warn!(error = %error, "battery service failed");
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<()> {
        tracing::debug!("battery service started");
        let upower = UPowerProxy::new(&self.conn).await?;
        let on_battery = upower.on_battery().await.unwrap_or(false);
        let device_paths = upower.enumerate_devices().await?;

        tracing::info!(
            devices = device_paths.len(),
            on_battery,
            "battery: enumerating UPower devices"
        );
        let mut battery_path: Option<OwnedObjectPath> = None;
        let mut state = self.state_tx.borrow().clone();
        state.devices.clear();

        for path in &device_paths {
            let dev = UPowerDeviceProxy::builder(&self.conn)
                .path(path.as_str())?
                .build()
                .await?;
            let type_id = dev.device_type().await.unwrap_or(0);
            if type_id == DeviceType::Battery as u32 && battery_path.is_none() {
                battery_path = Some(path.clone());
            }
            state.devices.push(BatteryDevice {
                path: path.to_string(),
                device_type: DeviceType::from(type_id),
                model: dev.model().await.unwrap_or_default(),
                percentage: dev.percentage().await.unwrap_or(0.0),
                state: BatteryState::from(dev.state().await.unwrap_or(0)),
                icon_name: dev.icon_name().await.unwrap_or_default(),
            });
        }

        let Some(bat_path) = battery_path else {
            tracing::warn!("battery: no battery found");
            state.status = BatteryStatus::default();
            self.change_state(state);
            cancel.cancelled().await;
            return Ok(());
        };

        let bat = UPowerDeviceProxy::builder(&self.conn)
            .path(bat_path.as_str())
            .map(|builder| builder.build())?
            .await?;

        self.read_state(&bat, on_battery, &mut state).await;
        tracing::info!(
            battery = %bat_path,
            percentage = state.status.percentage,
            state = ?state.status.state,
            model = %state.status.model,
            "battery: initial state"
        );

        let bat_props = zbus::fdo::PropertiesProxy::builder(&self.conn)
            .destination("org.freedesktop.UPower")?
            .path(bat_path.as_str())?
            .build()
            .await?;

        let mut bat_changes = bat_props.receive_properties_changed().await?;
        let mut upower_changes = upower.receive_on_battery_changed().await;

        self.change_state(state.clone());

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    break;
                },
                Some(_) = bat_changes.next() => {
                    let on_battery = upower.on_battery().await.unwrap_or(state.status.on_battery);
                    if self.read_state(&bat, on_battery, &mut state).await {
                        self.change_state(state.clone());
                    }
                }
                Some(_) = upower_changes.next() => {
                    let on_battery = upower.on_battery().await.unwrap_or(state.status.on_battery);
                    if self.read_state(&bat, on_battery, &mut state).await {
                        self.change_state(state.clone());
                    }
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(Command::Refresh)) => {
                        let on_battery = upower.on_battery().await.unwrap_or(state.status.on_battery);
                        if self.read_state(&bat, on_battery, &mut state).await {
                            self.change_state(state.clone());
                        }
                    }
                    Some(ServiceCommand::Control(_)) => {}
                    None => break,
                },
            }
        }
        tracing::debug!("battery service stopped");
        Ok(())
    }

    async fn read_state(
        &mut self,
        bat: &UPowerDeviceProxy<'_>,
        on_battery: bool,
        state: &mut State,
    ) -> bool {
        let next = BatteryStatus {
            present: true,
            device_type: DeviceType::from(bat.device_type().await.unwrap_or(0)),
            model: bat.model().await.unwrap_or_default(),
            percentage: bat.percentage().await.unwrap_or(0.0) as u8,
            state: BatteryState::from(bat.state().await.unwrap_or(0)),
            icon_name: bat
                .icon_name()
                .await
                .unwrap_or_else(|_| "battery-missing-symbolic".into()),
            on_battery,
            time_to_empty: bat.time_to_empty().await.unwrap_or(0),
            time_to_full: bat.time_to_full().await.unwrap_or(0),
            energy_rate: bat.energy_rate().await.unwrap_or(0.0),
            capacity: bat.capacity().await.unwrap_or(100.0),
            charge_threshold: self.read_charge_threshold(state),
        };

        let changed = should_emit_status(&state.status, &next);
        state.status = next;
        changed
    }

    fn read_charge_threshold(&mut self, state: &mut State) -> u32 {
        if state.threshold_path.is_none() {
            state.threshold_path = threshold_path();
        }

        match state
            .threshold_path
            .as_ref()
            .and_then(read_charge_threshold_from_path)
        {
            Some(value) => value,
            None => {
                state.threshold_path = threshold_path();
                state
                    .threshold_path
                    .as_ref()
                    .and_then(read_charge_threshold_from_path)
                    .unwrap_or(0)
            }
        }
    }
}

fn read_charge_threshold_from_path(path: &PathBuf) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn should_emit_status(previous: &BatteryStatus, next: &BatteryStatus) -> bool {
    previous != next
}

fn threshold_path() -> Option<PathBuf> {
    let dir = std::fs::read_dir("/sys/class/power_supply/").ok()?;
    for entry in dir.flatten() {
        let path = entry.path().join("charge_control_end_threshold");
        if path.exists() {
            return Some(path);
        }
    }
    None
}
