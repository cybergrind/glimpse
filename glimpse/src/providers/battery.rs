use std::path::PathBuf;

use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::zvariant::OwnedObjectPath;

use crate::dbus::DbusPropertyGroup;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceType {
    LinePower,
    Battery,
    Ups,
    Monitor,
    Mouse,
    Keyboard,
    Pda,
    Phone,
    #[default]
    Unknown,
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

#[derive(Debug, Clone, Serialize, Default)]
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

#[derive(Debug, Clone, Serialize, Default)]
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

#[derive(Debug, Clone, Serialize, Default)]
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
    status: BatteryStatus,
    devices: Vec<BatteryDevice>,
}

impl BatteryProvider {
    pub fn new() -> Self {
        Self {
            status: BatteryStatus::default(),
            devices: Vec::new(),
        }
    }

    pub async fn run(
        &mut self,
        conn: zbus::Connection,
        events: mpsc::Sender<BatteryEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let upower = DbusPropertyGroup::new(
            &conn,
            "org.freedesktop.UPower",
            "/org/freedesktop/UPower",
            "org.freedesktop.UPower",
        )
        .await?;

        let on_battery: bool = upower.get("OnBattery").await.unwrap_or(false);
        let device_paths: Vec<OwnedObjectPath> = upower.call("EnumerateDevices", &()).await?;
        tracing::info!(
            devices = device_paths.len(),
            on_battery,
            "battery: enumerating UPower devices"
        );

        let mut battery_path: Option<String> = None;
        self.devices.clear();

        for path in &device_paths {
            let dev = DbusPropertyGroup::new(
                &conn,
                "org.freedesktop.UPower",
                path.as_str(),
                "org.freedesktop.UPower.Device",
            )
            .await?;
            let type_id: u32 = dev.get("Type").await.unwrap_or(0);
            if type_id == DeviceType::Battery as u32 && battery_path.is_none() {
                battery_path = Some(path.to_string());
            }
            self.devices.push(BatteryDevice {
                path: path.to_string(),
                device_type: DeviceType::from(type_id),
                model: dev.get("Model").await.unwrap_or_default(),
                percentage: dev.get("Percentage").await.unwrap_or(0.0),
                state: BatteryState::from(dev.get::<u32>("State").await.unwrap_or(0)),
                icon_name: dev.get("IconName").await.unwrap_or_default(),
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

        let bat = DbusPropertyGroup::new(
            &conn,
            "org.freedesktop.UPower",
            &bat_path,
            "org.freedesktop.UPower.Device",
        )
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

        let mut bat_changes = bat.stream_changes().await?;
        let mut upower_changes = upower.stream_changes().await?;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                change = bat_changes.next() => {
                    if change.is_none() { break; }
                    let on_battery: bool = upower.get("OnBattery").await.unwrap_or(self.status.on_battery);
                    self.read_state(&bat, on_battery).await;
                    if events.send(BatteryEvent::StatusChanged(self.status.clone())).await.is_err() { break; }
                }
                change = upower_changes.next() => {
                    if change.is_none() { break; }
                    let on_battery: bool = upower.get("OnBattery").await.unwrap_or(self.status.on_battery);
                    self.read_state(&bat, on_battery).await;
                    if events.send(BatteryEvent::StatusChanged(self.status.clone())).await.is_err() { break; }
                }
            }
        }

        Ok(())
    }

    async fn read_state(&mut self, bat: &DbusPropertyGroup, on_battery: bool) {
        self.status = BatteryStatus {
            present: true,
            device_type: DeviceType::from(bat.get::<u32>("Type").await.unwrap_or(0)),
            model: bat.get("Model").await.unwrap_or_default(),
            percentage: bat.get::<f64>("Percentage").await.unwrap_or(0.0) as u8,
            state: BatteryState::from(bat.get::<u32>("State").await.unwrap_or(0)),
            icon_name: bat
                .get("IconName")
                .await
                .unwrap_or_else(|| "battery-missing-symbolic".into()),
            on_battery,
            time_to_empty: bat.get("TimeToEmpty").await.unwrap_or(0),
            time_to_full: bat.get("TimeToFull").await.unwrap_or(0),
            energy_rate: bat.get("EnergyRate").await.unwrap_or(0.0),
            capacity: bat.get("Capacity").await.unwrap_or(100.0),
            charge_threshold: get_charge_threshold(),
        };
    }
}

pub fn get_charge_threshold() -> u32 {
    threshold_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

pub fn set_charge_threshold(value: u32) -> anyhow::Result<()> {
    if value == 0 || value > 100 {
        return Err(anyhow::anyhow!("threshold must be 1-100"));
    }

    if let Some(path) = threshold_path() {
        if std::fs::write(&path, value.to_string()).is_ok() {
            return Ok(());
        }
    }

    let output = std::process::Command::new("pkexec")
        .arg("/usr/lib/glimpse/glimpse-battery-helper")
        .arg(value.to_string())
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run pkexec: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow::anyhow!(
            "failed to set threshold: {}",
            stderr.trim()
        ))
    }
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
