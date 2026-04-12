use std::{collections::HashMap, fmt, time::Duration};

use anyhow::anyhow;
use futures_util::{StreamExt, future};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::{
    MatchRule, MessageStream,
    message::Type,
    zvariant::{ObjectPath, OwnedValue, Value},
};

use crate::dbus::network_manager::{
    AccessPointProxy, ActiveConnectionProxy, DeviceProxy, DeviceWiredProxy, DeviceWirelessProxy,
    Ip4ConfigProxy, NetworkManagerProxy, SettingsConnectionProxy, SettingsProxy,
};

const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const LISTENER_DEBOUNCE: Duration = Duration::from_millis(300);
const NM_UPDATE2_FLAG_TO_DISK: u32 = 0x1;

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct NetworkStatus {
    pub connectivity: String,
    pub enabled: bool,
    pub wifi_enabled: bool,
    pub wifi_hw_enabled: bool,
    pub primary_connection: String,
    pub primary_type: String,
    pub metered: bool,
    pub speed: u32,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct WifiAccessPoint {
    pub path: String,
    pub ssid: String,
    pub strength: u8,
    pub frequency: u32,
    pub security: String,
    pub connected: bool,
    pub saved: bool,
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct NetworkConnection {
    pub active_path: String,
    pub id: String,
    pub uuid: String,
    pub connection_type: String,
    pub device: String,
    pub state: String,
    pub failure: Option<NetworkFailureClassification>,
    pub vpn: bool,
    pub ip4_address: Option<String>,
    pub gateway: Option<String>,
    pub dns: Vec<String>,
    pub speed: u32,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct NetworkDevice {
    pub interface: String,
    pub device_type: String,
    pub state: String,
    pub failure: Option<NetworkFailureClassification>,
    pub speed: u32,
    pub carrier: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct SavedVpn {
    pub id: String,
    pub uuid: String,
    pub connection_type: String,
    pub active: bool,
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum NetworkFailureClassification {
    AuthenticationFailed,
    MissingSecrets,
    Timeout,
    NetworkNotFound,
    ConfigurationFailed,
    ConnectionRemoved,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiActivationTarget {
    pub active_path: String,
    pub connection_uuid: Option<String>,
    pub settings_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SavedWifiProfile {
    id: String,
    uuid: String,
    ssid: String,
    has_inline_secret: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkSnapshot {
    pub status: NetworkStatus,
    pub wifi_access_points: Vec<WifiAccessPoint>,
    pub connections: Vec<NetworkConnection>,
    pub devices: Vec<NetworkDevice>,
    pub saved_vpns: Vec<SavedVpn>,
}

impl NetworkSnapshot {
    fn new(
        status: NetworkStatus,
        mut wifi_access_points: Vec<WifiAccessPoint>,
        mut connections: Vec<NetworkConnection>,
        mut devices: Vec<NetworkDevice>,
        mut saved_vpns: Vec<SavedVpn>,
    ) -> Self {
        wifi_access_points.sort_by(|left, right| {
            right
                .connected
                .cmp(&left.connected)
                .then(right.saved.cmp(&left.saved))
                .then(right.strength.cmp(&left.strength))
                .then(left.ssid.cmp(&right.ssid))
        });
        connections.sort_by(|left, right| left.id.cmp(&right.id));
        devices.sort_by(|left, right| left.interface.cmp(&right.interface));
        saved_vpns.sort_by(|left, right| left.id.cmp(&right.id));

        Self {
            status,
            wifi_access_points,
            connections,
            devices,
            saved_vpns,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkChangeReason {
    PropertiesChanged,
    DeviceAdded,
    DeviceRemoved,
    AccessPointAdded,
    AccessPointRemoved,
    Mixed,
}

impl fmt::Display for NetworkChangeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PropertiesChanged => "properties-changed",
            Self::DeviceAdded => "device-added",
            Self::DeviceRemoved => "device-removed",
            Self::AccessPointAdded => "access-point-added",
            Self::AccessPointRemoved => "access-point-removed",
            Self::Mixed => "mixed",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkProviderEvent {
    Changed { reason: NetworkChangeReason },
}

#[derive(Clone)]
pub struct NetworkProvider {
    conn: zbus::Connection,
}

impl NetworkProvider {
    pub fn new(conn: zbus::Connection) -> Self {
        Self { conn }
    }

    pub async fn scan(&self) -> anyhow::Result<NetworkSnapshot> {
        let manager = self.manager_proxy().await?;
        let mut status = NetworkStatus {
            connectivity: connectivity_str(manager.connectivity().await.unwrap_or(0)).into(),
            enabled: manager.networking_enabled().await.unwrap_or(false),
            wifi_enabled: manager.wireless_enabled().await.unwrap_or(false),
            wifi_hw_enabled: manager.wireless_hardware_enabled().await.unwrap_or(false),
            metered: matches!(manager.metered().await.unwrap_or(0), 1 | 3),
            ..NetworkStatus::default()
        };

        let primary_path = manager
            .primary_connection()
            .await
            .map(|path| path.to_string())
            .unwrap_or_default();
        if is_real_path(&primary_path) {
            let active = self.active_connection_proxy(&primary_path).await?;
            status.primary_connection = active.id().await.unwrap_or_default();
            status.primary_type =
                connection_type_str(&active.kind().await.unwrap_or_default()).into();
        }

        let devices = self.read_devices(&mut status).await?;
        let connections = self.read_connections(&mut status).await?;
        let wifi_access_points = self.read_access_points(&status, &connections).await?;
        let saved_vpns = self.read_saved_vpns(&connections).await?;
        resolve_icon(&mut status, &connections, &wifi_access_points);

        Ok(NetworkSnapshot::new(
            status,
            wifi_access_points,
            connections,
            devices,
            saved_vpns,
        ))
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<NetworkProviderEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let mut properties = self.match_stream("PropertiesChanged").await?;
        let mut device_added = self.match_stream("DeviceAdded").await?;
        let mut device_removed = self.match_stream("DeviceRemoved").await?;
        let mut ap_added = self.match_stream("AccessPointAdded").await?;
        let mut ap_removed = self.match_stream("AccessPointRemoved").await?;

        let mut pending_reason: Option<NetworkChangeReason> = None;
        let mut debounce_deadline: Option<tokio::time::Instant> = None;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                message = properties.next() => {
                    match message {
                        Some(Ok(message)) if is_network_manager_message(&message) => {
                            pending_reason = Some(merge_change_reason(pending_reason, NetworkChangeReason::PropertiesChanged));
                            debounce_deadline = Some(tokio::time::Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => tracing::warn!(error = %error, "network provider: properties stream error"),
                        None => break,
                    }
                }
                message = device_added.next() => {
                    match message {
                        Some(Ok(message)) if is_network_manager_message(&message) => {
                            pending_reason = Some(merge_change_reason(pending_reason, NetworkChangeReason::DeviceAdded));
                            debounce_deadline = Some(tokio::time::Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => tracing::warn!(error = %error, "network provider: device-added stream error"),
                        None => break,
                    }
                }
                message = device_removed.next() => {
                    match message {
                        Some(Ok(message)) if is_network_manager_message(&message) => {
                            pending_reason = Some(merge_change_reason(pending_reason, NetworkChangeReason::DeviceRemoved));
                            debounce_deadline = Some(tokio::time::Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => tracing::warn!(error = %error, "network provider: device-removed stream error"),
                        None => break,
                    }
                }
                message = ap_added.next() => {
                    match message {
                        Some(Ok(message)) if is_network_manager_message(&message) => {
                            pending_reason = Some(merge_change_reason(pending_reason, NetworkChangeReason::AccessPointAdded));
                            debounce_deadline = Some(tokio::time::Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => tracing::warn!(error = %error, "network provider: ap-added stream error"),
                        None => break,
                    }
                }
                message = ap_removed.next() => {
                    match message {
                        Some(Ok(message)) if is_network_manager_message(&message) => {
                            pending_reason = Some(merge_change_reason(pending_reason, NetworkChangeReason::AccessPointRemoved));
                            debounce_deadline = Some(tokio::time::Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => tracing::warn!(error = %error, "network provider: ap-removed stream error"),
                        None => break,
                    }
                }
                _ = async {
                    match debounce_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => future::pending::<()>().await,
                    }
                }, if debounce_deadline.is_some() => {
                    let reason = pending_reason.take().unwrap_or(NetworkChangeReason::PropertiesChanged);
                    debounce_deadline = None;
                    let _ = events.send(NetworkProviderEvent::Changed { reason }).await;
                }
            }
        }

        Ok(())
    }

    pub async fn set_wifi_enabled(&self, enabled: bool) -> anyhow::Result<()> {
        self.manager_proxy()
            .await?
            .set_wireless_enabled(enabled)
            .await
            .map_err(Into::into)
    }

    pub async fn request_scan(&self) -> anyhow::Result<()> {
        let device_paths = self.manager_proxy().await?.get_devices().await?;
        for device_path in device_paths {
            let device = self.device_proxy(device_path.as_str()).await?;
            if device.device_type().await.unwrap_or(0) != 2 {
                continue;
            }
            let wireless = self.wireless_device_proxy(device_path.as_str()).await?;
            let _ = wireless.request_scan(HashMap::new()).await;
        }
        Ok(())
    }

    pub async fn connect_access_point(
        &self,
        ssid: &str,
        access_point_path: &str,
        password: Option<&str>,
    ) -> anyhow::Result<WifiActivationTarget> {
        let manager = self.manager_proxy().await?;
        let device_paths = manager.get_devices().await?;
        let mut wifi_device_path = None;

        for device_path in device_paths {
            let device = self.device_proxy(device_path.as_str()).await?;
            if device.device_type().await.unwrap_or(0) != 2 {
                continue;
            }
            let wireless = self.wireless_device_proxy(device_path.as_str()).await?;
            let access_points = wireless.get_all_access_points().await.unwrap_or_default();
            for candidate in access_points {
                if candidate.as_str() == access_point_path {
                    wifi_device_path = Some(device_path.to_string());
                    break;
                }
            }
            if wifi_device_path.is_some() {
                break;
            }
        }

        let wifi_device_path = wifi_device_path.ok_or_else(|| anyhow!("no wifi device found"))?;

        let mut settings: HashMap<String, HashMap<String, OwnedValue>> = HashMap::new();
        let mut connection_settings = HashMap::new();
        connection_settings.insert("type".into(), owned_value("802-11-wireless"));
        settings.insert("connection".into(), connection_settings);

        let mut wifi_settings = HashMap::new();
        wifi_settings.insert("ssid".into(), owned_value(ssid.as_bytes().to_vec()));
        settings.insert("802-11-wireless".into(), wifi_settings);

        if let Some(password) = password {
            let mut security_settings = HashMap::new();
            security_settings.insert("key-mgmt".into(), owned_value("wpa-psk"));
            security_settings.insert("psk".into(), owned_value(password.to_string()));
            settings.insert("802-11-wireless-security".into(), security_settings);

            if let Some(wifi_settings) = settings.get_mut("802-11-wireless") {
                wifi_settings.insert("security".into(), owned_value("802-11-wireless-security"));
            }
        }

        let device = ObjectPath::try_from(wifi_device_path.as_str())?;
        let access_point = ObjectPath::try_from(access_point_path)?;
        let mut options = HashMap::new();
        if password.is_some() {
            options.insert("persist".into(), owned_value("volatile"));
        }
        let (connection_path, active_path, _) = manager
            .add_and_activate_connection2(settings, device, access_point, options)
            .await?;
        Ok(WifiActivationTarget {
            active_path: active_path.to_string(),
            connection_uuid: self
                .connection_uuid_for_settings_path(connection_path.as_str())
                .await?,
            settings_path: connection_path.to_string(),
        })
    }

    pub async fn connect_uuid(&self, uuid: &str) -> anyhow::Result<()> {
        let settings = self.settings_proxy().await?;
        let mut connection_path = settings.get_connection_by_uuid(uuid).await?;
        let mut connection_settings = self
            .settings_connection_proxy(connection_path.as_str())
            .await?
            .get_settings()
            .await
            .unwrap_or_default();
        if let Some(ssid) = saved_wifi_ssid(&connection_settings) {
            if let Some(preferred_uuid) = self.preferred_saved_wifi_uuid(&ssid).await? {
                if preferred_uuid != uuid {
                    connection_path = settings.get_connection_by_uuid(&preferred_uuid).await?;
                    connection_settings = self
                        .settings_connection_proxy(connection_path.as_str())
                        .await?
                        .get_settings()
                        .await
                        .unwrap_or_default();
                }
            }
        }
        let manager = self.manager_proxy().await?;

        let connection = ObjectPath::try_from(connection_path.as_str())?;
        let (device, specific_object) = match saved_wifi_ssid(&connection_settings) {
            Some(ssid) => self.resolve_wifi_activation_target(&ssid).await?,
            None => {
                let empty = ObjectPath::try_from("/")?;
                (empty.clone(), empty)
            }
        };
        let _ = manager
            .activate_connection(connection, device, specific_object)
            .await?;
        Ok(())
    }

    pub async fn disconnect(&self, uuid: &str) -> anyhow::Result<()> {
        let active_connections = self.manager_proxy().await?.active_connections().await?;
        for connection_path in active_connections {
            let connection = self
                .active_connection_proxy(connection_path.as_str())
                .await?;
            if connection.uuid().await.unwrap_or_default() == uuid {
                let active_object = ObjectPath::try_from(connection_path.as_str())?;
                self.manager_proxy()
                    .await?
                    .deactivate_connection(active_object)
                    .await?;
                return Ok(());
            }
        }

        tracing::info!(
            uuid,
            "network provider: disconnect requested for non-active connection"
        );
        Ok(())
    }

    pub async fn forget(&self, uuid: &str) -> anyhow::Result<()> {
        let settings = self.settings_proxy().await?;
        let connection_path = settings.get_connection_by_uuid(uuid).await?;
        let connection_settings = self
            .settings_connection_proxy(connection_path.as_str())
            .await?
            .get_settings()
            .await
            .unwrap_or_default();

        if let Some(ssid) = saved_wifi_ssid(&connection_settings) {
            for profile_path in self.wifi_profile_paths_for_ssid(&ssid).await? {
                self.settings_connection_proxy(profile_path.as_str())
                    .await?
                    .delete()
                    .await?;
            }
            return Ok(());
        }

        self.settings_connection_proxy(connection_path.as_str())
            .await?
            .delete()
            .await
            .map_err(Into::into)
    }

    pub async fn delete_connection_path(&self, path: &str) -> anyhow::Result<()> {
        self.settings_connection_proxy(path)
            .await?
            .delete()
            .await
            .map_err(Into::into)
    }

    pub async fn save_connection_path(&self, path: &str) -> anyhow::Result<()> {
        self.settings_connection_proxy(path)
            .await?
            .update2(HashMap::new(), NM_UPDATE2_FLAG_TO_DISK, HashMap::new())
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn read_devices(&self, status: &mut NetworkStatus) -> anyhow::Result<Vec<NetworkDevice>> {
        let mut devices = Vec::new();
        let device_paths = self.manager_proxy().await?.get_devices().await?;
        for path in device_paths {
            let device = self.device_proxy(path.as_str()).await?;
            let device_type = device_type_str(device.device_type().await.unwrap_or(0));
            if device_type == "other" {
                continue;
            }

            let state = device.state().await.unwrap_or(0);
            let state_reason = device.state_reason().await.unwrap_or(0);
            let interface = device.interface().await.unwrap_or_default();
            let (speed, carrier) = match device_type {
                "ethernet" => {
                    let wired = self.wired_device_proxy(path.as_str()).await?;
                    (wired.speed().await.unwrap_or(0), wired.carrier().await.ok())
                }
                "wifi" => {
                    let wireless = self.wireless_device_proxy(path.as_str()).await?;
                    (wireless.bitrate().await.unwrap_or(0) / 1000, None)
                }
                _ => (0, None),
            };

            if device_type == "ethernet" && state == 100 {
                status.speed = speed;
            }

            devices.push(NetworkDevice {
                interface,
                device_type: device_type.into(),
                state: device_state_str(state).into(),
                failure: device_failure_classification(device_type, state, state_reason),
                speed,
                carrier,
            });
        }

        Ok(devices)
    }

    async fn read_connections(
        &self,
        status: &mut NetworkStatus,
    ) -> anyhow::Result<Vec<NetworkConnection>> {
        let mut connections = Vec::new();
        let active_paths = self.manager_proxy().await?.active_connections().await?;
        for path in active_paths {
            let active = self.active_connection_proxy(path.as_str()).await?;
            let id = active.id().await.unwrap_or_default();
            let uuid = active.uuid().await.unwrap_or_default();
            let raw_type = active.kind().await.unwrap_or_default();
            let state = active.state().await.unwrap_or(0);
            let state_reason = active.state_reason().await.unwrap_or(0);
            let vpn = active.vpn().await.unwrap_or(false);

            let mut device_name = String::new();
            let mut speed = 0;
            if let Some(device_path) = active.devices().await.unwrap_or_default().first() {
                let device = self.device_proxy(device_path.as_str()).await?;
                device_name = device.interface().await.unwrap_or_default();
                speed = match device_type_str(device.device_type().await.unwrap_or(0)) {
                    "ethernet" => self
                        .wired_device_proxy(device_path.as_str())
                        .await?
                        .speed()
                        .await
                        .unwrap_or(0),
                    "wifi" => {
                        self.wireless_device_proxy(device_path.as_str())
                            .await?
                            .bitrate()
                            .await
                            .unwrap_or(0)
                            / 1000
                    }
                    _ => 0,
                };
            }

            let mut ip4_address = None;
            let mut gateway = None;
            let mut dns = Vec::new();
            if let Ok(ip4_path) = active.ip4_config().await {
                if is_real_path(ip4_path.as_str()) {
                    let ip4 = self.ip4_config_proxy(ip4_path.as_str()).await?;
                    gateway = ip4.gateway().await.ok();
                    if let Some(first) = ip4.address_data().await.unwrap_or_default().first() {
                        if let Some(address) = first.get("address").and_then(owned_value_to_string)
                        {
                            ip4_address = Some(address);
                        }
                    }
                    for entry in ip4.nameserver_data().await.unwrap_or_default() {
                        if let Some(address) = entry.get("address").and_then(owned_value_to_string)
                        {
                            dns.push(address);
                        }
                    }
                }
            }

            let connection_type = connection_type_str(&raw_type);
            if connection_type == "wifi" && state == 2 {
                status.speed = speed;
            }

            connections.push(NetworkConnection {
                active_path: path.to_string(),
                id,
                uuid,
                connection_type: connection_type.into(),
                device: device_name,
                state: connection_state_str(state).into(),
                failure: active_connection_failure_classification(state, state_reason),
                vpn,
                ip4_address,
                gateway,
                dns,
                speed,
            });
        }

        Ok(connections)
    }

    async fn read_access_points(
        &self,
        status: &NetworkStatus,
        connections: &[NetworkConnection],
    ) -> anyhow::Result<Vec<WifiAccessPoint>> {
        if !status.wifi_enabled {
            return Ok(Vec::new());
        }

        let connected_ssids: HashMap<String, String> = connections
            .iter()
            .filter(|connection| {
                connection.connection_type == "wifi" && connection.state == "activated"
            })
            .map(|connection| (connection.id.clone(), connection.uuid.clone()))
            .collect();
        let saved_wifi = self.read_saved_wifi_profiles().await?;
        let device_paths = self.manager_proxy().await?.get_devices().await?;

        let mut access_points = Vec::new();
        for device_path in device_paths {
            let device = self.device_proxy(device_path.as_str()).await?;
            if device.device_type().await.unwrap_or(0) != 2 {
                continue;
            }
            let wireless = self.wireless_device_proxy(device_path.as_str()).await?;
            for access_point_path in wireless.get_all_access_points().await.unwrap_or_default() {
                let access_point = self.access_point_proxy(access_point_path.as_str()).await?;
                let ssid = String::from_utf8_lossy(&access_point.ssid().await.unwrap_or_default())
                    .to_string();
                if ssid.is_empty() {
                    continue;
                }

                let strength = access_point.strength().await.unwrap_or(0);
                let frequency = access_point.frequency().await.unwrap_or(0);
                let flags = access_point.flags().await.unwrap_or(0);
                let wpa_flags = access_point.wpa_flags().await.unwrap_or(0);
                let rsn_flags = access_point.rsn_flags().await.unwrap_or(0);

                let connected = connected_ssids.contains_key(&ssid);
                let saved_profiles = saved_wifi.get(&ssid);
                let saved_uuid = preferred_saved_wifi_profile(
                    saved_profiles.map(|profiles| profiles.as_slice()),
                    &ssid,
                )
                .map(|profile| profile.uuid.clone());
                let saved = saved_profiles.is_some() || connected;
                let uuid = if connected {
                    connected_ssids.get(&ssid).cloned()
                } else {
                    saved_uuid
                };

                access_points.push(WifiAccessPoint {
                    path: access_point_path.to_string(),
                    ssid,
                    strength,
                    frequency,
                    security: ap_security(flags, wpa_flags, rsn_flags).into(),
                    connected,
                    saved,
                    uuid,
                });
            }
        }

        Ok(access_points)
    }

    async fn read_saved_wifi_profiles(
        &self,
    ) -> anyhow::Result<HashMap<String, Vec<SavedWifiProfile>>> {
        let mut saved: HashMap<String, Vec<SavedWifiProfile>> = HashMap::new();
        for path in self.settings_proxy().await?.list_connections().await? {
            let settings = self
                .settings_connection_proxy(path.as_str())
                .await?
                .get_settings()
                .await
                .unwrap_or_default();

            let Some(connection_section) = settings.get("connection") else {
                continue;
            };
            let connection_type = connection_section
                .get("type")
                .and_then(owned_value_to_string)
                .unwrap_or_default();
            if connection_type != "802-11-wireless" {
                continue;
            }

            let id = connection_section
                .get("id")
                .and_then(owned_value_to_string)
                .unwrap_or_default();
            let uuid = connection_section
                .get("uuid")
                .and_then(owned_value_to_string)
                .unwrap_or_default();
            let has_inline_secret = settings
                .get("802-11-wireless-security")
                .and_then(|security| security.get("psk"))
                .and_then(owned_value_to_string)
                .map(|psk| !psk.is_empty())
                .unwrap_or(false);
            let Some(wifi_section) = settings.get("802-11-wireless") else {
                continue;
            };
            let Some(ssid) = wifi_section.get("ssid").and_then(owned_value_to_ssid) else {
                continue;
            };
            if !ssid.is_empty() && !uuid.is_empty() {
                saved
                    .entry(ssid.clone())
                    .or_default()
                    .push(SavedWifiProfile {
                        id,
                        uuid,
                        ssid,
                        has_inline_secret,
                    });
            }
        }

        Ok(saved)
    }

    async fn read_saved_vpns(
        &self,
        connections: &[NetworkConnection],
    ) -> anyhow::Result<Vec<SavedVpn>> {
        let active_vpns: HashMap<String, String> = connections
            .iter()
            .filter(|connection| {
                connection.vpn
                    || connection.connection_type == "vpn"
                    || connection.connection_type == "wireguard"
            })
            .map(|connection| (connection.uuid.clone(), connection.state.clone()))
            .collect();

        let mut saved_vpns = Vec::new();
        for path in self.settings_proxy().await?.list_connections().await? {
            let settings = self
                .settings_connection_proxy(path.as_str())
                .await?
                .get_settings()
                .await
                .unwrap_or_default();

            let Some(connection_section) = settings.get("connection") else {
                continue;
            };
            let connection_type = connection_section
                .get("type")
                .and_then(owned_value_to_string)
                .unwrap_or_default();
            if connection_type != "vpn" && connection_type != "wireguard" {
                continue;
            }

            let id = connection_section
                .get("id")
                .and_then(owned_value_to_string)
                .unwrap_or_default();
            let uuid = connection_section
                .get("uuid")
                .and_then(owned_value_to_string)
                .unwrap_or_default();
            let active_state = active_vpns.get(&uuid);

            saved_vpns.push(SavedVpn {
                id,
                uuid,
                connection_type: connection_type_str(&connection_type).into(),
                active: active_state.is_some(),
                state: active_state.cloned(),
            });
        }

        Ok(saved_vpns)
    }

    async fn preferred_saved_wifi_uuid(&self, ssid: &str) -> anyhow::Result<Option<String>> {
        let profiles = self.read_saved_wifi_profiles().await?;
        Ok(preferred_saved_wifi_profile(
            profiles.get(ssid).map(|profiles| profiles.as_slice()),
            ssid,
        )
        .map(|profile| profile.uuid.clone()))
    }

    async fn connection_uuid_for_settings_path(
        &self,
        path: &str,
    ) -> anyhow::Result<Option<String>> {
        let settings = match self.settings_connection_proxy(path).await {
            Ok(proxy) => proxy.get_settings().await.unwrap_or_default(),
            Err(_) => return Ok(None),
        };

        Ok(settings
            .get("connection")
            .and_then(|connection| connection.get("uuid"))
            .and_then(owned_value_to_string)
            .filter(|uuid| !uuid.is_empty()))
    }

    async fn wifi_profile_paths_for_ssid(&self, ssid: &str) -> anyhow::Result<Vec<String>> {
        let mut paths = Vec::new();
        for path in self.settings_proxy().await?.list_connections().await? {
            let settings = self
                .settings_connection_proxy(path.as_str())
                .await?
                .get_settings()
                .await
                .unwrap_or_default();
            if saved_wifi_ssid(&settings).as_deref() == Some(ssid) {
                paths.push(path.to_string());
            }
        }
        Ok(paths)
    }

    async fn resolve_wifi_activation_target(
        &self,
        ssid: &str,
    ) -> anyhow::Result<(ObjectPath<'static>, ObjectPath<'static>)> {
        let device_paths = self.manager_proxy().await?.get_devices().await?;
        for device_path in device_paths {
            let device = self.device_proxy(device_path.as_str()).await?;
            if device.device_type().await.unwrap_or(0) != 2 {
                continue;
            }

            let wireless = self.wireless_device_proxy(device_path.as_str()).await?;
            for access_point_path in wireless.get_all_access_points().await.unwrap_or_default() {
                let access_point = self.access_point_proxy(access_point_path.as_str()).await?;
                let ap_ssid =
                    String::from_utf8_lossy(&access_point.ssid().await.unwrap_or_default())
                        .to_string();
                if ap_ssid == ssid {
                    return Ok((
                        ObjectPath::try_from(device_path.to_string())?,
                        ObjectPath::try_from(access_point_path.to_string())?,
                    ));
                }
            }
        }

        let empty = ObjectPath::try_from("/")?;
        Ok((empty.clone(), empty))
    }

    async fn manager_proxy(&self) -> anyhow::Result<NetworkManagerProxy<'_>> {
        NetworkManagerProxy::new(&self.conn)
            .await
            .map_err(Into::into)
    }

    async fn device_proxy<'a>(&'a self, path: &'a str) -> anyhow::Result<DeviceProxy<'a>> {
        DeviceProxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn wired_device_proxy<'a>(
        &'a self,
        path: &'a str,
    ) -> anyhow::Result<DeviceWiredProxy<'a>> {
        DeviceWiredProxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn wireless_device_proxy<'a>(
        &'a self,
        path: &'a str,
    ) -> anyhow::Result<DeviceWirelessProxy<'a>> {
        DeviceWirelessProxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn access_point_proxy<'a>(
        &'a self,
        path: &'a str,
    ) -> anyhow::Result<AccessPointProxy<'a>> {
        AccessPointProxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn active_connection_proxy<'a>(
        &'a self,
        path: &'a str,
    ) -> anyhow::Result<ActiveConnectionProxy<'a>> {
        ActiveConnectionProxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn ip4_config_proxy<'a>(&'a self, path: &'a str) -> anyhow::Result<Ip4ConfigProxy<'a>> {
        Ip4ConfigProxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn settings_proxy(&self) -> anyhow::Result<SettingsProxy<'_>> {
        SettingsProxy::new(&self.conn).await.map_err(Into::into)
    }

    async fn settings_connection_proxy<'a>(
        &'a self,
        path: &'a str,
    ) -> anyhow::Result<SettingsConnectionProxy<'a>> {
        SettingsConnectionProxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn match_stream(&self, member: &'static str) -> anyhow::Result<MessageStream> {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(NM_SERVICE)?
            .member(member)?
            .build();

        MessageStream::for_match_rule(rule, &self.conn, None)
            .await
            .map_err(Into::into)
    }
}

fn connectivity_str(value: u32) -> &'static str {
    match value {
        1 => "none",
        2 => "portal",
        3 => "limited",
        4 => "full",
        _ => "unknown",
    }
}

fn device_state_str(value: u32) -> &'static str {
    match value {
        100 => "connected",
        20 => "unavailable",
        110 => "deactivating",
        30..=90 => "connecting",
        _ => "disconnected",
    }
}

fn device_type_str(value: u32) -> &'static str {
    match value {
        1 => "ethernet",
        2 => "wifi",
        29 => "wireguard",
        _ => "other",
    }
}

fn connection_state_str(value: u32) -> &'static str {
    match value {
        1 => "activating",
        2 => "activated",
        3 => "deactivating",
        _ => "unknown",
    }
}

fn device_failure_classification(
    device_type: &str,
    state: u32,
    state_reason: u32,
) -> Option<NetworkFailureClassification> {
    if state != 120 {
        return None;
    }

    match state_reason {
        4 => Some(NetworkFailureClassification::ConfigurationFailed),
        7 => Some(NetworkFailureClassification::MissingSecrets),
        8 => Some(NetworkFailureClassification::Disconnected),
        9..=10 => Some(NetworkFailureClassification::AuthenticationFailed),
        11 => Some(NetworkFailureClassification::Timeout),
        39 => Some(NetworkFailureClassification::Disconnected),
        53 if device_type == "wifi" => Some(NetworkFailureClassification::NetworkNotFound),
        _ => None,
    }
}

fn active_connection_failure_classification(
    state: u32,
    state_reason: u32,
) -> Option<NetworkFailureClassification> {
    if state == 2 {
        return None;
    }

    match state_reason {
        3 => Some(NetworkFailureClassification::Disconnected),
        5 => Some(NetworkFailureClassification::ConfigurationFailed),
        6 => Some(NetworkFailureClassification::Timeout),
        9 => Some(NetworkFailureClassification::MissingSecrets),
        10 => Some(NetworkFailureClassification::AuthenticationFailed),
        11 => Some(NetworkFailureClassification::ConnectionRemoved),
        _ => None,
    }
}

fn connection_type_str(value: &str) -> &'static str {
    match value {
        "802-11-wireless" => "wifi",
        "802-3-ethernet" => "ethernet",
        "vpn" => "vpn",
        "wireguard" => "wireguard",
        _ => "other",
    }
}

fn ap_security(flags: u32, wpa_flags: u32, rsn_flags: u32) -> &'static str {
    if wpa_flags == 0 && rsn_flags == 0 {
        if flags & 0x01 != 0 {
            return "wep";
        }
        return "open";
    }
    if rsn_flags & 0x400 != 0 {
        return "wpa3";
    }
    if rsn_flags & 0x200 != 0 {
        return "enterprise";
    }
    if rsn_flags & 0x100 != 0 {
        return "wpa2";
    }
    if wpa_flags & 0x100 != 0 {
        return "wpa";
    }
    "secured"
}

fn wifi_icon(strength: u8) -> &'static str {
    match strength {
        75..=100 => "network-wireless-signal-excellent-symbolic",
        50..=74 => "network-wireless-signal-good-symbolic",
        25..=49 => "network-wireless-signal-ok-symbolic",
        1..=24 => "network-wireless-signal-weak-symbolic",
        _ => "network-wireless-signal-none-symbolic",
    }
}

fn resolve_icon(
    status: &mut NetworkStatus,
    connections: &[NetworkConnection],
    wifi_access_points: &[WifiAccessPoint],
) {
    if !status.enabled {
        status.icon = "network-offline-symbolic".into();
        return;
    }

    if connections
        .iter()
        .any(|connection| connection.connection_type == "wifi" && connection.state == "activated")
    {
        let strength = wifi_access_points
            .iter()
            .find(|access_point| access_point.connected)
            .map(|access_point| access_point.strength)
            .unwrap_or(0);
        status.icon = wifi_icon(strength).into();
        return;
    }

    if connections.iter().any(|connection| {
        connection.connection_type == "ethernet" && connection.state == "activated"
    }) {
        status.icon = "network-wired-symbolic".into();
        return;
    }

    if !status.wifi_enabled {
        status.icon = "network-wireless-disabled-symbolic".into();
        return;
    }

    status.icon = "network-offline-symbolic".into();
}

fn merge_change_reason(
    current: Option<NetworkChangeReason>,
    next: NetworkChangeReason,
) -> NetworkChangeReason {
    match current {
        None => next,
        Some(current) if current == next => current,
        Some(_) => NetworkChangeReason::Mixed,
    }
}

fn is_network_manager_message(message: &zbus::message::Message) -> bool {
    let header = message.header();
    let Some(path) = header.path() else {
        return false;
    };
    path.as_str().starts_with("/org/freedesktop/NetworkManager")
}

fn owned_value(value: impl Into<Value<'static>>) -> OwnedValue {
    value
        .into()
        .try_to_owned()
        .expect("network manager value conversion should not fail")
}

fn owned_value_to_string(value: &OwnedValue) -> Option<String> {
    String::try_from(value.clone()).ok()
}

fn owned_value_to_ssid(value: &OwnedValue) -> Option<String> {
    Vec::<u8>::try_from(value.clone())
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
}

fn saved_wifi_ssid(settings: &HashMap<String, HashMap<String, OwnedValue>>) -> Option<String> {
    let connection_section = settings.get("connection")?;
    let connection_type = connection_section
        .get("type")
        .and_then(owned_value_to_string)?;
    if connection_type != "802-11-wireless" {
        return None;
    }

    settings
        .get("802-11-wireless")
        .and_then(|wifi_section| wifi_section.get("ssid"))
        .and_then(owned_value_to_ssid)
}

fn preferred_saved_wifi_profile<'a>(
    profiles: Option<&'a [SavedWifiProfile]>,
    ssid: &str,
) -> Option<&'a SavedWifiProfile> {
    let mut profiles = profiles?.iter().collect::<Vec<_>>();
    profiles.sort_by(|left, right| {
        right
            .has_inline_secret
            .cmp(&left.has_inline_secret)
            .then((right.id == ssid).cmp(&(left.id == ssid)))
            .then(left.id.len().cmp(&right.id.len()))
            .then(left.id.cmp(&right.id))
    });
    profiles.into_iter().next()
}

fn is_real_path(path: &str) -> bool {
    !path.is_empty() && path != "/"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connectivity_values() {
        assert_eq!(connectivity_str(0), "unknown");
        assert_eq!(connectivity_str(1), "none");
        assert_eq!(connectivity_str(2), "portal");
        assert_eq!(connectivity_str(3), "limited");
        assert_eq!(connectivity_str(4), "full");
        assert_eq!(connectivity_str(99), "unknown");
    }

    #[test]
    fn device_state_values() {
        assert_eq!(device_state_str(20), "unavailable");
        assert_eq!(device_state_str(30), "connecting");
        assert_eq!(device_state_str(50), "connecting");
        assert_eq!(device_state_str(90), "connecting");
        assert_eq!(device_state_str(100), "connected");
        assert_eq!(device_state_str(110), "deactivating");
        assert_eq!(device_state_str(0), "disconnected");
    }

    #[test]
    fn device_type_values() {
        assert_eq!(device_type_str(1), "ethernet");
        assert_eq!(device_type_str(2), "wifi");
        assert_eq!(device_type_str(29), "wireguard");
        assert_eq!(device_type_str(5), "other");
    }

    #[test]
    fn connection_state_values() {
        assert_eq!(connection_state_str(1), "activating");
        assert_eq!(connection_state_str(2), "activated");
        assert_eq!(connection_state_str(3), "deactivating");
        assert_eq!(connection_state_str(0), "unknown");
    }

    #[test]
    fn wifi_device_failures_are_classified() {
        assert_eq!(
            device_failure_classification("wifi", 120, 7),
            Some(NetworkFailureClassification::MissingSecrets)
        );
        assert_eq!(
            device_failure_classification("wifi", 120, 8),
            Some(NetworkFailureClassification::Disconnected)
        );
        assert_eq!(
            device_failure_classification("wifi", 120, 9),
            Some(NetworkFailureClassification::AuthenticationFailed)
        );
        assert_eq!(
            device_failure_classification("wifi", 120, 53),
            Some(NetworkFailureClassification::NetworkNotFound)
        );
    }

    #[test]
    fn active_connection_failures_are_classified() {
        assert_eq!(
            active_connection_failure_classification(4, 9),
            Some(NetworkFailureClassification::MissingSecrets)
        );
        assert_eq!(
            active_connection_failure_classification(4, 10),
            Some(NetworkFailureClassification::AuthenticationFailed)
        );
        assert_eq!(
            active_connection_failure_classification(4, 6),
            Some(NetworkFailureClassification::Timeout)
        );
    }

    #[test]
    fn connection_type_mapping() {
        assert_eq!(connection_type_str("802-11-wireless"), "wifi");
        assert_eq!(connection_type_str("802-3-ethernet"), "ethernet");
        assert_eq!(connection_type_str("vpn"), "vpn");
        assert_eq!(connection_type_str("wireguard"), "wireguard");
        assert_eq!(connection_type_str("bridge"), "other");
    }

    #[test]
    fn saved_wifi_ssid_extracts_wireless_profile_name() {
        let mut connection = HashMap::new();
        connection.insert("type".into(), owned_value("802-11-wireless"));

        let mut wifi = HashMap::new();
        wifi.insert("ssid".into(), owned_value(b"Skylink".to_vec()));

        let settings = HashMap::from([
            ("connection".into(), connection),
            ("802-11-wireless".into(), wifi),
        ]);

        assert_eq!(saved_wifi_ssid(&settings).as_deref(), Some("Skylink"));
    }

    #[test]
    fn saved_wifi_ssid_ignores_non_wifi_profiles() {
        let mut connection = HashMap::new();
        connection.insert("type".into(), owned_value("vpn"));

        let settings = HashMap::from([("connection".into(), connection)]);

        assert_eq!(saved_wifi_ssid(&settings), None);
    }

    #[test]
    fn preferred_saved_wifi_profile_prefers_inline_secret() {
        let profiles = vec![
            SavedWifiProfile {
                id: "Skylink 1".into(),
                uuid: "uuid-1".into(),
                ssid: "Skylink".into(),
                has_inline_secret: false,
            },
            SavedWifiProfile {
                id: "Skylink".into(),
                uuid: "uuid-2".into(),
                ssid: "Skylink".into(),
                has_inline_secret: true,
            },
        ];

        assert_eq!(
            preferred_saved_wifi_profile(Some(&profiles), "Skylink")
                .map(|profile| profile.uuid.as_str()),
            Some("uuid-2")
        );
    }

    #[test]
    fn preferred_saved_wifi_profile_prefers_exact_ssid_name_when_secrets_match() {
        let profiles = vec![
            SavedWifiProfile {
                id: "Skylink 2".into(),
                uuid: "uuid-1".into(),
                ssid: "Skylink".into(),
                has_inline_secret: false,
            },
            SavedWifiProfile {
                id: "Skylink".into(),
                uuid: "uuid-2".into(),
                ssid: "Skylink".into(),
                has_inline_secret: false,
            },
        ];

        assert_eq!(
            preferred_saved_wifi_profile(Some(&profiles), "Skylink")
                .map(|profile| profile.uuid.as_str()),
            Some("uuid-2")
        );
    }

    #[test]
    fn security_open() {
        assert_eq!(ap_security(0, 0, 0), "open");
    }

    #[test]
    fn security_wep() {
        assert_eq!(ap_security(0x01, 0, 0), "wep");
    }

    #[test]
    fn security_wpa_psk() {
        assert_eq!(ap_security(0, 0x100, 0), "wpa");
    }

    #[test]
    fn security_wpa2_psk() {
        assert_eq!(ap_security(0, 0, 0x100), "wpa2");
    }

    #[test]
    fn security_wpa3() {
        assert_eq!(ap_security(0, 0, 0x400), "wpa3");
    }

    #[test]
    fn security_enterprise() {
        assert_eq!(ap_security(0, 0, 0x200), "enterprise");
    }

    #[test]
    fn wifi_icon_thresholds() {
        assert_eq!(wifi_icon(100), "network-wireless-signal-excellent-symbolic");
        assert_eq!(wifi_icon(55), "network-wireless-signal-good-symbolic");
        assert_eq!(wifi_icon(30), "network-wireless-signal-ok-symbolic");
        assert_eq!(wifi_icon(5), "network-wireless-signal-weak-symbolic");
        assert_eq!(wifi_icon(0), "network-wireless-signal-none-symbolic");
    }

    #[test]
    fn icon_offline_when_disabled() {
        let mut status = NetworkStatus {
            enabled: false,
            ..NetworkStatus::default()
        };
        resolve_icon(&mut status, &[], &[]);
        assert_eq!(status.icon, "network-offline-symbolic");
    }

    #[test]
    fn icon_wifi_from_connected_access_point() {
        let mut status = NetworkStatus {
            enabled: true,
            wifi_enabled: true,
            ..NetworkStatus::default()
        };
        resolve_icon(
            &mut status,
            &[NetworkConnection {
                connection_type: "wifi".into(),
                state: "activated".into(),
                ..NetworkConnection::default()
            }],
            &[WifiAccessPoint {
                ssid: "Home".into(),
                strength: 82,
                connected: true,
                ..WifiAccessPoint::default()
            }],
        );
        assert_eq!(status.icon, "network-wireless-signal-excellent-symbolic");
    }

    #[test]
    fn icon_wired_when_ethernet_active() {
        let mut status = NetworkStatus {
            enabled: true,
            wifi_enabled: true,
            ..NetworkStatus::default()
        };
        resolve_icon(
            &mut status,
            &[NetworkConnection {
                connection_type: "ethernet".into(),
                state: "activated".into(),
                ..NetworkConnection::default()
            }],
            &[],
        );
        assert_eq!(status.icon, "network-wired-symbolic");
    }

    #[test]
    fn icon_wifi_disabled() {
        let mut status = NetworkStatus {
            enabled: true,
            wifi_enabled: false,
            ..NetworkStatus::default()
        };
        resolve_icon(&mut status, &[], &[]);
        assert_eq!(status.icon, "network-wireless-disabled-symbolic");
    }

    #[test]
    fn snapshot_sorts_lists() {
        let snapshot = NetworkSnapshot::new(
            NetworkStatus::default(),
            vec![
                WifiAccessPoint {
                    ssid: "b".into(),
                    strength: 10,
                    ..WifiAccessPoint::default()
                },
                WifiAccessPoint {
                    ssid: "a".into(),
                    connected: true,
                    strength: 5,
                    ..WifiAccessPoint::default()
                },
            ],
            vec![
                NetworkConnection {
                    id: "z".into(),
                    ..NetworkConnection::default()
                },
                NetworkConnection {
                    id: "a".into(),
                    ..NetworkConnection::default()
                },
            ],
            vec![
                NetworkDevice {
                    interface: "wlan0".into(),
                    ..NetworkDevice::default()
                },
                NetworkDevice {
                    interface: "enp0s1".into(),
                    ..NetworkDevice::default()
                },
            ],
            vec![
                SavedVpn {
                    id: "z".into(),
                    ..SavedVpn::default()
                },
                SavedVpn {
                    id: "a".into(),
                    ..SavedVpn::default()
                },
            ],
        );

        assert_eq!(snapshot.wifi_access_points[0].ssid, "a");
        assert_eq!(snapshot.connections[0].id, "a");
        assert_eq!(snapshot.devices[0].interface, "enp0s1");
        assert_eq!(snapshot.saved_vpns[0].id, "a");
    }
}
