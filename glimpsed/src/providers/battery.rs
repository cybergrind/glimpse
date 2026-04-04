use std::pin::Pin;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::zvariant::OwnedObjectPath;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};
use crate::providers::dbus_props::DbusPropertyGroup;

const NAME: &str = "battery";
const TOPICS: &[&str] = &["battery.status", "battery.devices"];
const METHODS: &[&str] = &["battery.set_charge_threshold", "battery.get_charge_threshold"];

#[derive(Debug, Clone, Serialize, Default)]
struct BatteryStatus {
    present: bool,
    device_type: &'static str,
    model: String,
    percentage: u8,
    state: &'static str,
    icon_name: String,
    on_battery: bool,
    time_to_empty: i64,
    time_to_full: i64,
    energy_rate: f64,
    capacity: f64,
    /// Charge end threshold (0 = not supported). Read from sysfs.
    charge_threshold: u32,
}

#[derive(Debug, Clone, Serialize, Default)]
struct BatteryDevice {
    path: String,
    device_type: &'static str,
    model: String,
    percentage: f64,
    state: &'static str,
    icon_name: String,
}

struct BatteryProvider {
    status: BatteryStatus,
    devices: Vec<BatteryDevice>,
}

impl Provider for BatteryProvider {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }

    fn run(
        &mut self,
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            let conn = zbus::Connection::system().await?;

            let upower = DbusPropertyGroup::new(
                &conn,
                "org.freedesktop.UPower",
                "/org/freedesktop/UPower",
                "org.freedesktop.UPower",
            )
            .await?;

            let on_battery: bool = upower.get("OnBattery").await.unwrap_or(false);

            // Enumerate all devices and find the primary battery.
            let device_paths: Vec<OwnedObjectPath> = upower.call("EnumerateDevices", &()).await?;
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
                if type_id == 2 && battery_path.is_none() {
                    battery_path = Some(path.to_string());
                }
                self.devices.push(BatteryDevice {
                    path: path.to_string(),
                    device_type: device_type_str(type_id),
                    model: dev.get("Model").await.unwrap_or_default(),
                    percentage: dev.get("Percentage").await.unwrap_or(0.0),
                    state: state_str(dev.get("State").await.unwrap_or(0)),
                    icon_name: dev.get("IconName").await.unwrap_or_default(),
                });
            }

            let Some(bat_path) = battery_path else {
                self.status = BatteryStatus::default();
                let _ = events
                    .send(ProviderEvent {
                        topic: "battery.status".into(),
                        data: json!({"present": false}),
                    })
                    .await;
                // No battery — just handle requests until cancelled.
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => return Ok(()),
                        req = requests.recv() => {
                            let Some(req) = req else { return Ok(()) };
                            self.handle_request(req);
                        }
                    }
                }
            };

            let bat = DbusPropertyGroup::new(
                &conn,
                "org.freedesktop.UPower",
                &bat_path,
                "org.freedesktop.UPower.Device",
            )
            .await?;

            // Read initial state (snapshot is served by broker on subscribe).
            self.read_state(&bat, on_battery).await;

            // Stream property changes from both device and UPower.
            let mut bat_changes = bat.stream_changes().await?;
            let mut upower_changes = upower.stream_changes().await?;

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req);
                    }
                    change = bat_changes.next() => {
                        let Some(_) = change else { break };
                        let on_battery: bool = upower.get_uncached("OnBattery").await.unwrap_or(self.status.on_battery);
                        self.read_state(&bat, on_battery).await;
                        if events.send(ProviderEvent {
                            topic: "battery.status".into(),
                            data: serde_json::to_value(&self.status)?,
                        }).await.is_err() { break; }
                    }
                    change = upower_changes.next() => {
                        let Some(_) = change else { break };
                        let on_battery: bool = upower.get_uncached("OnBattery").await.unwrap_or(self.status.on_battery);
                        self.read_state(&bat, on_battery).await;
                        if events.send(ProviderEvent {
                            topic: "battery.status".into(),
                            data: serde_json::to_value(&self.status)?,
                        }).await.is_err() { break; }
                    }
                }
            }

            Ok(())
        })
    }
}

fn device_type_str(t: u32) -> &'static str {
    match t {
        1 => "line-power",
        2 => "battery",
        3 => "ups",
        4 => "monitor",
        5 => "mouse",
        6 => "keyboard",
        7 => "pda",
        8 => "phone",
        _ => "unknown",
    }
}

fn state_str(s: u32) -> &'static str {
    match s {
        1 => "charging",
        2 => "discharging",
        3 => "empty",
        4 => "fully-charged",
        5 => "pending-charge",
        6 => "pending-discharge",
        _ => "unknown",
    }
}

impl BatteryProvider {
    async fn read_state(&mut self, bat: &DbusPropertyGroup, on_battery: bool) {
        let model: String = bat.get_uncached("Model").await.unwrap_or_default();
        let pct: f64 = bat.get_uncached("Percentage").await.unwrap_or(0.0);
        let state_u32: u32 = bat.get_uncached("State").await.unwrap_or(0);
        let icon: String = bat
            .get_uncached("IconName")
            .await
            .unwrap_or_else(|| "battery-missing-symbolic".into());
        let tte: i64 = bat.get_uncached("TimeToEmpty").await.unwrap_or(0);
        let ttf: i64 = bat.get_uncached("TimeToFull").await.unwrap_or(0);
        let rate: f64 = bat.get_uncached("EnergyRate").await.unwrap_or(0.0);
        let cap: f64 = bat.get_uncached("Capacity").await.unwrap_or(100.0);

        self.status = BatteryStatus {
            present: true,
            device_type: device_type_str(bat.get_uncached::<u32>("Type").await.unwrap_or(0)),
            model,
            percentage: pct as u8,
            state: state_str(state_u32),
            icon_name: icon,
            on_battery,
            time_to_empty: tte,
            time_to_full: ttf,
            energy_rate: rate,
            capacity: cap,
            charge_threshold: read_charge_threshold(),
        };
    }

    fn handle_request(&self, req: ProviderRequest) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "battery.status" => serde_json::to_value(&self.status).ok(),
                    "battery.devices" => serde_json::to_value(&self.devices).ok(),
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call {
                method,
                params,
                reply,
            } => {
                let result = match method.as_str() {
                    "battery.get_charge_threshold" => {
                        let val = read_charge_threshold();
                        if val > 0 {
                            Ok(json!({"threshold": val, "supported": true}))
                        } else {
                            Ok(json!({"threshold": 0, "supported": false}))
                        }
                    }
                    "battery.set_charge_threshold" => {
                        let threshold = params["threshold"].as_u64().unwrap_or(0) as u32;
                        if threshold == 0 || threshold > 100 {
                            Err(anyhow::anyhow!("threshold must be 1-100"))
                        } else {
                            write_charge_threshold(threshold)
                                .map(|()| json!({"threshold": threshold}))
                        }
                    }
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                let _ = reply.send(result);
            }
        }
    }
}

/// Find the sysfs path for charge_control_end_threshold.
fn threshold_path() -> Option<std::path::PathBuf> {
    let dir = std::fs::read_dir("/sys/class/power_supply/").ok()?;
    for entry in dir.flatten() {
        let path = entry.path().join("charge_control_end_threshold");
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn read_charge_threshold() -> u32 {
    threshold_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn write_charge_threshold(value: u32) -> anyhow::Result<()> {
    // Try direct write first (works if udev rule grants permission).
    if let Some(path) = threshold_path() {
        if std::fs::write(&path, value.to_string()).is_ok() {
            return Ok(());
        }
    }

    // Fall back to pkexec with polkit helper.
    let output = std::process::Command::new("pkexec")
        .arg("/usr/lib/glimpse/glimpse-battery-helper")
        .arg(value.to_string())
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run pkexec: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow::anyhow!("failed to set threshold: {}", stderr.trim()))
    }
}

pub struct BatteryProviderFactory;

impl ProviderFactory for BatteryProviderFactory {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }
    fn create(&self) -> Box<dyn Provider> {
        Box::new(BatteryProvider {
            status: BatteryStatus::default(),
            devices: Vec::new(),
        })
    }
}
