use std::path::PathBuf;

use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::zvariant::OwnedObjectPath;

use crate::dbus::upower::{UPowerDeviceProxy, UPowerProxy};

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
pub enum BatteryEvent {
    StatusChanged(BatteryStatus),
    DevicesChanged(Vec<BatteryDevice>),
}

pub struct BatteryProvider {
    conn: zbus::Connection,
    status: BatteryStatus,
    devices: Vec<BatteryDevice>,
    threshold_path: Option<PathBuf>,
}

impl BatteryProvider {
    pub fn new(conn: zbus::Connection) -> Self {
        Self {
            conn,
            status: BatteryStatus::default(),
            devices: Vec::new(),
            threshold_path: None,
        }
    }

    pub async fn run(
        &mut self,
        events: mpsc::Sender<BatteryEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let upower = UPowerProxy::new(&self.conn).await?;

        let on_battery = upower.on_battery().await.unwrap_or(false);
        let device_paths = upower.enumerate_devices().await?;
        tracing::info!(
            devices = device_paths.len(),
            on_battery,
            "battery: enumerating UPower devices"
        );

        let mut battery_path: Option<OwnedObjectPath> = None;
        self.devices.clear();

        for path in &device_paths {
            let dev = UPowerDeviceProxy::builder(&self.conn)
                .path(path.as_str())?
                .build()
                .await?;
            let type_id = dev.device_type().await.unwrap_or(0);
            if type_id == DeviceType::Battery as u32 && battery_path.is_none() {
                battery_path = Some(path.clone());
            }
            self.devices.push(BatteryDevice {
                path: path.to_string(),
                device_type: DeviceType::from(type_id),
                model: dev.model().await.unwrap_or_default(),
                percentage: dev.percentage().await.unwrap_or(0.0),
                state: BatteryState::from(dev.state().await.unwrap_or(0)),
                icon_name: dev.icon_name().await.unwrap_or_default(),
            });
        }

        let _ = events
            .send(BatteryEvent::DevicesChanged(self.devices.clone()))
            .await;

        let Some(bat_path) = battery_path else {
            tracing::warn!("battery: no battery found");
            self.status = BatteryStatus::default();
            let _ = events
                .send(BatteryEvent::StatusChanged(self.status.clone()))
                .await;
            cancel.cancelled().await;
            return Ok(());
        };

        let bat = UPowerDeviceProxy::builder(&self.conn)
            .path(bat_path.as_str())?
            .build()
            .await?;

        self.read_state(&bat, on_battery).await;
        tracing::info!(
            battery = %bat_path,
            percentage = self.status.percentage,
            state = ?self.status.state,
            model = %self.status.model,
            "battery: initial state"
        );
        let _ = events
            .send(BatteryEvent::StatusChanged(self.status.clone()))
            .await;

        let bat_props = zbus::fdo::PropertiesProxy::builder(&self.conn)
            .destination("org.freedesktop.UPower")?
            .path(bat_path.as_str())?
            .build()
            .await?;
        let mut bat_changes = bat_props.receive_properties_changed().await?;
        let mut upower_changes = upower.receive_on_battery_changed().await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(_) = bat_changes.next() => {
                    let on_battery = upower.on_battery().await.unwrap_or(self.status.on_battery);
                    if self.read_state(&bat, on_battery).await
                        && events.send(BatteryEvent::StatusChanged(self.status.clone())).await.is_err()
                    {
                        break;
                    }
                }
                Some(_) = upower_changes.next() => {
                    let on_battery = upower.on_battery().await.unwrap_or(self.status.on_battery);
                    if self.read_state(&bat, on_battery).await
                        && events.send(BatteryEvent::StatusChanged(self.status.clone())).await.is_err()
                    {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    async fn read_state(&mut self, bat: &UPowerDeviceProxy<'_>, on_battery: bool) -> bool {
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
            charge_threshold: self.read_charge_threshold(),
        };

        let changed = should_emit_status(&self.status, &next);
        self.status = next;
        changed
    }

    fn read_charge_threshold(&mut self) -> u32 {
        if self.threshold_path.is_none() {
            self.threshold_path = threshold_path();
        }

        match self
            .threshold_path
            .as_ref()
            .and_then(read_charge_threshold_from_path)
        {
            Some(value) => value,
            None => {
                self.threshold_path = threshold_path();
                self.threshold_path
                    .as_ref()
                    .and_then(read_charge_threshold_from_path)
                    .unwrap_or(0)
            }
        }
    }
}

pub fn get_charge_threshold() -> u32 {
    threshold_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::{BatteryState, BatteryStatus};

    #[test]
    fn should_emit_status_only_for_real_changes() {
        let previous = BatteryStatus {
            present: true,
            percentage: 55,
            state: BatteryState::Discharging,
            icon_name: "battery-good-symbolic".into(),
            on_battery: true,
            ..BatteryStatus::default()
        };

        assert!(!super::should_emit_status(&previous, &previous));

        let mut next = previous.clone();
        next.percentage = 54;
        assert!(super::should_emit_status(&previous, &next));
    }
}
