use std::{collections::HashMap, fmt, time::Duration};

use anyhow::anyhow;
use futures_util::{StreamExt, future};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
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
    pub device_path: String,
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
    pub settings_path: Option<String>,
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
    pub path: String,
    pub interface: String,
    pub device_type: String,
    pub state: String,
    pub failure: Option<NetworkFailureClassification>,
    pub speed: u32,
    pub carrier: Option<bool>,
    pub hardware_address: Option<String>,
    pub driver: Option<String>,
    pub managed: bool,
    pub mtu: Option<u32>,
    pub hotspot_supported: bool,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct SavedVpn {
    pub id: String,
    pub uuid: String,
    pub settings_path: String,
    pub connection_type: String,
    pub active: bool,
    pub state: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
pub enum NetworkIpMethod {
    #[default]
    Automatic,
    Manual,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct NetworkIpConfig {
    pub method: NetworkIpMethod,
    pub address: String,
    pub prefix: Option<u32>,
    pub gateway: String,
    pub dns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct NetworkConnectionConfig {
    pub id: String,
    pub uuid: String,
    pub settings_path: String,
    pub connection_type: String,
    pub interface_name: Option<String>,
    pub autoconnect: bool,
    pub ipv4: NetworkIpConfig,
    pub ipv6: NetworkIpConfig,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct HotspotConfig {
    pub id: String,
    pub uuid: Option<String>,
    pub settings_path: Option<String>,
    pub device_path: String,
    pub interface_name: String,
    pub active: bool,
    pub ssid: String,
    pub password: String,
    pub band: String,
}

#[derive(Debug, Clone, Copy, Serialize, Default, PartialEq, Eq)]
pub enum VpnTransportProtocol {
    Tcp,
    #[default]
    Udp,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct OpenVpnConfig {
    pub gateway: String,
    pub port: Option<u16>,
    pub protocol: VpnTransportProtocol,
    pub username: String,
    pub password: String,
    pub ca_cert: String,
    pub client_cert: String,
    pub private_key: String,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct WireGuardPeerConfig {
    pub public_key: String,
    pub preshared_key: String,
    pub endpoint: String,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct WireGuardConfig {
    pub private_key: String,
    pub listen_port: Option<u16>,
    pub mtu: Option<u32>,
    pub peers: Vec<WireGuardPeerConfig>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum VpnConfigKind {
    OpenVpn(OpenVpnConfig),
    WireGuard(WireGuardConfig),
}

impl Default for VpnConfigKind {
    fn default() -> Self {
        Self::OpenVpn(OpenVpnConfig::default())
    }
}

impl VpnConfigKind {
    pub fn connection_type(&self) -> &'static str {
        match self {
            Self::OpenVpn(_) => "vpn",
            Self::WireGuard(_) => "wireguard",
        }
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct VpnProfileConfig {
    pub id: String,
    pub uuid: Option<String>,
    pub settings_path: Option<String>,
    pub autoconnect: bool,
    pub interface_name: Option<String>,
    pub ipv4: NetworkIpConfig,
    pub ipv6: NetworkIpConfig,
    pub kind: VpnConfigKind,
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

    pub async fn load_connection_config(
        &self,
        uuid: &str,
    ) -> anyhow::Result<NetworkConnectionConfig> {
        let settings_path = self
            .settings_proxy()
            .await?
            .get_connection_by_uuid(uuid)
            .await?
            .to_string();
        self.load_connection_config_by_path(&settings_path).await
    }

    pub async fn load_connection_config_by_path(
        &self,
        settings_path: &str,
    ) -> anyhow::Result<NetworkConnectionConfig> {
        let settings = self
            .settings_connection_proxy(settings_path)
            .await?
            .get_settings()
            .await
            .unwrap_or_default();

        Ok(parse_connection_config(&settings, settings_path))
    }

    pub async fn apply_connection_config(
        &self,
        config: &NetworkConnectionConfig,
    ) -> anyhow::Result<()> {
        let proxy = self
            .settings_connection_proxy(&config.settings_path)
            .await?;
        let mut settings = proxy.get_settings().await.unwrap_or_default();

        let connection_section = settings.entry("connection".into()).or_default();
        connection_section.insert("id".into(), owned_value(config.id.clone()));
        connection_section.insert("uuid".into(), owned_value(config.uuid.clone()));
        connection_section.insert(
            "type".into(),
            owned_value(raw_connection_type(&config.connection_type)),
        );
        if let Some(interface_name) = &config.interface_name {
            if !interface_name.trim().is_empty() {
                connection_section
                    .insert("interface-name".into(), owned_value(interface_name.clone()));
            } else {
                connection_section.remove("interface-name");
            }
        }
        connection_section.insert("autoconnect".into(), owned_value(config.autoconnect));

        write_ip_config(
            settings.entry("ipv4".into()).or_default(),
            &config.ipv4,
            false,
        );
        write_ip_config(
            settings.entry("ipv6".into()).or_default(),
            &config.ipv6,
            true,
        );

        proxy
            .update2(settings, NM_UPDATE2_FLAG_TO_DISK, HashMap::new())
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    pub async fn load_hotspot_config(&self, device_path: &str) -> anyhow::Result<HotspotConfig> {
        let device = self.device_proxy(device_path).await?;
        let interface_name = device.interface().await.unwrap_or_default();
        let path = self.find_hotspot_profile_path(&interface_name).await?;
        let active_uuid = self.find_active_hotspot_uuid(&interface_name).await?;

        if let Some(settings_path) = path {
            let settings = self
                .settings_connection_proxy(settings_path.as_str())
                .await?
                .get_settings()
                .await
                .unwrap_or_default();
            let mut config =
                parse_hotspot_config(&settings, &settings_path, device_path, &interface_name);
            config.active = active_uuid
                .as_deref()
                .is_some_and(|uuid| config.uuid.as_deref() == Some(uuid));
            return Ok(config);
        }

        Ok(HotspotConfig {
            id: format!("Hotspot ({interface_name})"),
            uuid: None,
            settings_path: None,
            device_path: device_path.to_owned(),
            interface_name,
            active: false,
            ssid: "Glimpse Hotspot".into(),
            password: String::new(),
            band: "bg".into(),
        })
    }

    pub async fn apply_hotspot_config(
        &self,
        config: &HotspotConfig,
    ) -> anyhow::Result<HotspotConfig> {
        let settings = hotspot_settings_map(config);
        let settings_path = if let Some(path) = &config.settings_path {
            self.settings_connection_proxy(path)
                .await?
                .update2(settings, NM_UPDATE2_FLAG_TO_DISK, HashMap::new())
                .await?;
            path.clone()
        } else {
            self.settings_proxy()
                .await?
                .add_connection(settings)
                .await?
                .to_string()
        };

        self.load_hotspot_config(config.device_path.as_str())
            .await
            .or_else(|_| {
                Ok(HotspotConfig {
                    settings_path: Some(settings_path),
                    ..config.clone()
                })
            })
    }

    pub async fn set_hotspot_enabled(
        &self,
        config: &HotspotConfig,
        enabled: bool,
    ) -> anyhow::Result<()> {
        let saved = self.apply_hotspot_config(config).await?;
        let settings_path = saved
            .settings_path
            .clone()
            .ok_or_else(|| anyhow!("hotspot settings path missing"))?;

        if enabled {
            let manager = self.manager_proxy().await?;
            let connection = ObjectPath::try_from(settings_path.as_str())?;
            let device = ObjectPath::try_from(saved.device_path.as_str())?;
            let specific = ObjectPath::try_from("/")?;
            let _ = manager
                .activate_connection(connection, device, specific)
                .await?;
            return Ok(());
        }

        if let Some(uuid) = saved.uuid.as_deref() {
            self.disconnect(uuid).await?;
        }
        Ok(())
    }

    pub async fn load_vpn_profile(&self, settings_path: &str) -> anyhow::Result<VpnProfileConfig> {
        let settings = self
            .settings_connection_proxy(settings_path)
            .await?
            .get_settings()
            .await
            .unwrap_or_default();
        Ok(parse_vpn_profile_config(&settings, settings_path))
    }

    pub async fn create_vpn_profile(&self, config: &VpnProfileConfig) -> anyhow::Result<String> {
        let settings_path = self
            .settings_proxy()
            .await?
            .add_connection(vpn_profile_settings_map(config))
            .await?
            .to_string();
        Ok(settings_path)
    }

    pub async fn update_vpn_profile(&self, config: &VpnProfileConfig) -> anyhow::Result<()> {
        let settings_path = config
            .settings_path
            .as_deref()
            .ok_or_else(|| anyhow!("vpn settings path missing"))?;
        self.settings_connection_proxy(settings_path)
            .await?
            .update2(
                vpn_profile_settings_map(config),
                NM_UPDATE2_FLAG_TO_DISK,
                HashMap::new(),
            )
            .await
            .map(|_| ())
            .map_err(Into::into)
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

            let hardware_address = device
                .hw_address()
                .await
                .ok()
                .filter(|value| !value.is_empty());
            let driver = device.driver().await.ok().filter(|value| !value.is_empty());
            let managed = device.managed().await.unwrap_or(true);
            let mtu = device.mtu().await.ok().filter(|value| *value > 0);
            let hotspot_supported = if device_type == "wifi" {
                match self.wireless_device_proxy(path.as_str()).await {
                    Ok(wireless) => wireless
                        .wireless_capabilities()
                        .await
                        .map(|caps| caps & 0x00000040 != 0)
                        .unwrap_or(true),
                    Err(_) => true,
                }
            } else {
                false
            };

            devices.push(NetworkDevice {
                path: path.to_string(),
                interface,
                device_type: device_type.into(),
                state: device_state_str(state).into(),
                failure: device_failure_classification(device_type, state, state_reason),
                speed,
                carrier,
                hardware_address,
                driver,
                managed,
                mtu,
                hotspot_supported,
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
                settings_path: self.settings_path_for_uuid(&uuid).await.ok().flatten(),
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
                    device_path: device_path.to_string(),
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
                settings_path: path.to_string(),
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

    async fn settings_path_for_uuid(&self, uuid: &str) -> anyhow::Result<Option<String>> {
        if uuid.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(
            self.settings_proxy()
                .await?
                .get_connection_by_uuid(uuid)
                .await?
                .to_string(),
        ))
    }

    async fn find_hotspot_profile_path(
        &self,
        interface_name: &str,
    ) -> anyhow::Result<Option<String>> {
        for path in self.settings_proxy().await?.list_connections().await? {
            let settings = self
                .settings_connection_proxy(path.as_str())
                .await?
                .get_settings()
                .await
                .unwrap_or_default();
            if is_hotspot_profile(&settings, interface_name) {
                return Ok(Some(path.to_string()));
            }
        }
        Ok(None)
    }

    async fn find_active_hotspot_uuid(
        &self,
        interface_name: &str,
    ) -> anyhow::Result<Option<String>> {
        for connection in self.scan().await?.connections {
            if connection.connection_type != "wifi" {
                continue;
            }
            if connection.device != interface_name || connection.state != "activated" {
                continue;
            }
            if let Some(settings_path) = &connection.settings_path {
                let settings = self
                    .settings_connection_proxy(settings_path)
                    .await?
                    .get_settings()
                    .await
                    .unwrap_or_default();
                if is_hotspot_profile(&settings, interface_name) {
                    return Ok(Some(connection.uuid));
                }
            }
        }
        Ok(None)
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

fn parse_connection_config(
    settings: &HashMap<String, HashMap<String, OwnedValue>>,
    settings_path: &str,
) -> NetworkConnectionConfig {
    let connection = settings.get("connection");
    let id = connection
        .and_then(|section| section.get("id"))
        .and_then(owned_value_to_string)
        .unwrap_or_default();
    let uuid = connection
        .and_then(|section| section.get("uuid"))
        .and_then(owned_value_to_string)
        .unwrap_or_default();
    let connection_type = connection
        .and_then(|section| section.get("type"))
        .and_then(owned_value_to_string)
        .map(|value| connection_type_str(&value).to_owned())
        .unwrap_or_else(|| "other".into());
    let interface_name = connection
        .and_then(|section| section.get("interface-name"))
        .and_then(owned_value_to_string)
        .filter(|value| !value.is_empty());
    let autoconnect = connection
        .and_then(|section| section.get("autoconnect"))
        .and_then(owned_value_to_bool)
        .unwrap_or(true);

    NetworkConnectionConfig {
        id,
        uuid,
        settings_path: settings_path.to_owned(),
        connection_type,
        interface_name,
        autoconnect,
        ipv4: parse_ip_config(settings.get("ipv4"), false),
        ipv6: parse_ip_config(settings.get("ipv6"), true),
    }
}

fn parse_hotspot_config(
    settings: &HashMap<String, HashMap<String, OwnedValue>>,
    settings_path: &str,
    device_path: &str,
    interface_name: &str,
) -> HotspotConfig {
    let connection = settings.get("connection");
    let wifi = settings.get("802-11-wireless");
    let security = settings.get("802-11-wireless-security");

    HotspotConfig {
        id: connection
            .and_then(|section| section.get("id"))
            .and_then(owned_value_to_string)
            .unwrap_or_else(|| format!("Hotspot ({interface_name})")),
        uuid: connection
            .and_then(|section| section.get("uuid"))
            .and_then(owned_value_to_string)
            .filter(|value| !value.is_empty()),
        settings_path: Some(settings_path.to_owned()),
        device_path: device_path.to_owned(),
        interface_name: interface_name.to_owned(),
        active: false,
        ssid: wifi
            .and_then(|section| section.get("ssid"))
            .and_then(owned_value_to_ssid)
            .unwrap_or_else(|| "Glimpse Hotspot".into()),
        password: security
            .and_then(|section| section.get("psk"))
            .and_then(owned_value_to_string)
            .unwrap_or_default(),
        band: wifi
            .and_then(|section| section.get("band"))
            .and_then(owned_value_to_string)
            .unwrap_or_else(|| "bg".into()),
    }
}

fn parse_vpn_profile_config(
    settings: &HashMap<String, HashMap<String, OwnedValue>>,
    settings_path: &str,
) -> VpnProfileConfig {
    let connection = settings.get("connection");
    let id = connection
        .and_then(|section| section.get("id"))
        .and_then(owned_value_to_string)
        .unwrap_or_default();
    let uuid = connection
        .and_then(|section| section.get("uuid"))
        .and_then(owned_value_to_string)
        .filter(|value| !value.is_empty());
    let interface_name = connection
        .and_then(|section| section.get("interface-name"))
        .and_then(owned_value_to_string)
        .filter(|value| !value.is_empty());
    let autoconnect = connection
        .and_then(|section| section.get("autoconnect"))
        .and_then(owned_value_to_bool)
        .unwrap_or(true);

    VpnProfileConfig {
        id,
        uuid,
        settings_path: Some(settings_path.to_owned()),
        autoconnect,
        interface_name,
        ipv4: parse_ip_config(settings.get("ipv4"), false),
        ipv6: parse_ip_config(settings.get("ipv6"), true),
        kind: match connection
            .and_then(|section| section.get("type"))
            .and_then(owned_value_to_string)
            .as_deref()
        {
            Some("wireguard") => VpnConfigKind::WireGuard(parse_wireguard_config(settings)),
            _ => VpnConfigKind::OpenVpn(parse_openvpn_config(settings)),
        },
    }
}

fn parse_openvpn_config(settings: &HashMap<String, HashMap<String, OwnedValue>>) -> OpenVpnConfig {
    let vpn = settings.get("vpn");
    let data = vpn
        .and_then(|section| section.get("data"))
        .and_then(owned_value_to_string_map)
        .unwrap_or_default();
    let secrets = vpn
        .and_then(|section| section.get("secrets"))
        .and_then(owned_value_to_string_map)
        .unwrap_or_default();

    OpenVpnConfig {
        gateway: data.get("remote").cloned().unwrap_or_default(),
        port: data.get("port").and_then(|value| value.parse::<u16>().ok()),
        protocol: match data.get("proto").map(String::as_str) {
            Some("tcp") => VpnTransportProtocol::Tcp,
            _ => VpnTransportProtocol::Udp,
        },
        username: vpn
            .and_then(|section| section.get("user-name"))
            .and_then(owned_value_to_string)
            .unwrap_or_default(),
        password: secrets.get("password").cloned().unwrap_or_default(),
        ca_cert: data.get("ca").cloned().unwrap_or_default(),
        client_cert: data.get("cert").cloned().unwrap_or_default(),
        private_key: data.get("key").cloned().unwrap_or_default(),
    }
}

fn parse_wireguard_config(
    settings: &HashMap<String, HashMap<String, OwnedValue>>,
) -> WireGuardConfig {
    let section = settings.get("wireguard");
    let peers = section
        .and_then(|section| section.get("peers"))
        .and_then(owned_value_to_address_data)
        .unwrap_or_default()
        .into_iter()
        .map(|peer| WireGuardPeerConfig {
            public_key: peer
                .get("public-key")
                .and_then(owned_value_to_string)
                .unwrap_or_default(),
            preshared_key: peer
                .get("preshared-key")
                .and_then(owned_value_to_string)
                .unwrap_or_default(),
            endpoint: peer
                .get("endpoint")
                .and_then(owned_value_to_string)
                .unwrap_or_default(),
            allowed_ips: peer
                .get("allowed-ips")
                .and_then(owned_value_to_string_vec)
                .unwrap_or_default(),
            persistent_keepalive: peer
                .get("persistent-keepalive")
                .and_then(|value| owned_value_to_u16(value)),
        })
        .collect();

    WireGuardConfig {
        private_key: section
            .and_then(|section| section.get("private-key"))
            .and_then(owned_value_to_string)
            .unwrap_or_default(),
        listen_port: section
            .and_then(|section| section.get("listen-port"))
            .and_then(owned_value_to_u16),
        mtu: section
            .and_then(|section| section.get("mtu"))
            .and_then(|value| owned_value_to_u32(value, false)),
        peers,
    }
}

fn parse_ip_config(section: Option<&HashMap<String, OwnedValue>>, ipv6: bool) -> NetworkIpConfig {
    let method = section
        .and_then(|section| section.get("method"))
        .and_then(owned_value_to_string)
        .as_deref()
        .map(parse_ip_method)
        .unwrap_or(NetworkIpMethod::Automatic);

    let mut address = String::new();
    let mut prefix = None;
    if let Some(first) = section
        .and_then(|section| section.get("address-data"))
        .and_then(owned_value_to_address_data)
        .and_then(|entries| entries.into_iter().next())
    {
        address = first
            .get("address")
            .and_then(owned_value_to_string)
            .unwrap_or_default();
        prefix = first
            .get("prefix")
            .and_then(|value| owned_value_to_u32(value, ipv6));
    }

    let dns = section
        .and_then(|section| section.get("dns-data"))
        .and_then(owned_value_to_string_vec)
        .unwrap_or_default();

    NetworkIpConfig {
        method,
        address,
        prefix,
        gateway: section
            .and_then(|section| section.get("gateway"))
            .and_then(owned_value_to_string)
            .unwrap_or_default(),
        dns,
    }
}

fn parse_ip_method(value: &str) -> NetworkIpMethod {
    match value {
        "manual" => NetworkIpMethod::Manual,
        "disabled" | "ignore" => NetworkIpMethod::Disabled,
        _ => NetworkIpMethod::Automatic,
    }
}

fn raw_connection_type(value: &str) -> String {
    match value {
        "wifi" => "802-11-wireless".into(),
        "ethernet" => "802-3-ethernet".into(),
        "vpn" => "vpn".into(),
        "wireguard" => "wireguard".into(),
        _ => value.to_owned(),
    }
}

fn raw_ip_method(method: NetworkIpMethod, ipv6: bool) -> &'static str {
    match method {
        NetworkIpMethod::Automatic => "auto",
        NetworkIpMethod::Manual => "manual",
        NetworkIpMethod::Disabled if ipv6 => "ignore",
        NetworkIpMethod::Disabled => "disabled",
    }
}

fn write_ip_config(
    section: &mut HashMap<String, OwnedValue>,
    config: &NetworkIpConfig,
    ipv6: bool,
) {
    section.insert(
        "method".into(),
        owned_value(raw_ip_method(config.method, ipv6)),
    );

    if matches!(config.method, NetworkIpMethod::Manual) && !config.address.trim().is_empty() {
        let mut address: HashMap<String, OwnedValue> = HashMap::new();
        address.insert("address".into(), owned_value(config.address.clone()));
        address.insert(
            "prefix".into(),
            owned_value(config.prefix.unwrap_or(if ipv6 { 64 } else { 24 })),
        );
        section.insert("address-data".into(), owned_value(vec![address]));
    } else {
        section.remove("address-data");
    }

    if matches!(config.method, NetworkIpMethod::Manual) && !config.gateway.trim().is_empty() {
        section.insert("gateway".into(), owned_value(config.gateway.clone()));
    } else {
        section.remove("gateway");
    }

    let dns = config
        .dns
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if dns.is_empty() || !matches!(config.method, NetworkIpMethod::Manual) {
        section.remove("dns-data");
    } else {
        section.insert("dns-data".into(), owned_value(dns));
    }
}

fn hotspot_settings_map(config: &HotspotConfig) -> HashMap<String, HashMap<String, OwnedValue>> {
    let mut connection = HashMap::new();
    connection.insert("id".into(), owned_value(config.id.clone()));
    connection.insert("type".into(), owned_value("802-11-wireless"));
    connection.insert("autoconnect".into(), owned_value(false));
    connection.insert(
        "interface-name".into(),
        owned_value(config.interface_name.clone()),
    );
    if let Some(uuid) = &config.uuid {
        if !uuid.is_empty() {
            connection.insert("uuid".into(), owned_value(uuid.clone()));
        }
    }

    let mut wifi = HashMap::new();
    wifi.insert("ssid".into(), owned_value(config.ssid.as_bytes().to_vec()));
    wifi.insert("mode".into(), owned_value("ap"));
    if !config.band.trim().is_empty() {
        wifi.insert("band".into(), owned_value(config.band.clone()));
    }

    let mut ipv4 = HashMap::new();
    ipv4.insert("method".into(), owned_value("shared"));

    let mut ipv6 = HashMap::new();
    ipv6.insert("method".into(), owned_value("ignore"));

    let mut settings = HashMap::from([
        ("connection".into(), connection),
        ("802-11-wireless".into(), wifi),
        ("ipv4".into(), ipv4),
        ("ipv6".into(), ipv6),
    ]);

    if !config.password.trim().is_empty() {
        let mut security = HashMap::new();
        security.insert("key-mgmt".into(), owned_value("wpa-psk"));
        security.insert("psk".into(), owned_value(config.password.clone()));
        settings.insert("802-11-wireless-security".into(), security);
        if let Some(wifi) = settings.get_mut("802-11-wireless") {
            wifi.insert("security".into(), owned_value("802-11-wireless-security"));
        }
    }

    settings
}

fn vpn_profile_settings_map(
    config: &VpnProfileConfig,
) -> HashMap<String, HashMap<String, OwnedValue>> {
    let mut connection = HashMap::new();
    let uuid = config
        .uuid
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    connection.insert("id".into(), owned_value(config.id.clone()));
    connection.insert(
        "type".into(),
        owned_value(raw_connection_type(config.kind.connection_type())),
    );
    connection.insert("autoconnect".into(), owned_value(config.autoconnect));
    connection.insert("uuid".into(), owned_value(uuid));
    if let Some(interface_name) = &config.interface_name {
        if !interface_name.trim().is_empty() {
            connection.insert("interface-name".into(), owned_value(interface_name.clone()));
        }
    }

    let mut settings = HashMap::from([
        ("connection".into(), connection),
        ("ipv4".into(), HashMap::new()),
        ("ipv6".into(), HashMap::new()),
    ]);
    write_ip_config(
        settings.get_mut("ipv4").expect("ipv4 should exist"),
        &config.ipv4,
        false,
    );
    write_ip_config(
        settings.get_mut("ipv6").expect("ipv6 should exist"),
        &config.ipv6,
        true,
    );

    match &config.kind {
        VpnConfigKind::OpenVpn(openvpn) => {
            let mut vpn = HashMap::new();
            vpn.insert(
                "service-type".into(),
                owned_value("org.freedesktop.NetworkManager.openvpn"),
            );
            if !openvpn.username.trim().is_empty() {
                vpn.insert("user-name".into(), owned_value(openvpn.username.clone()));
            }

            let mut data = HashMap::new();
            if !openvpn.gateway.trim().is_empty() {
                data.insert("remote".to_string(), openvpn.gateway.clone());
            }
            if let Some(port) = openvpn.port {
                data.insert("port".to_string(), port.to_string());
            }
            data.insert(
                "proto".to_string(),
                match openvpn.protocol {
                    VpnTransportProtocol::Tcp => "tcp".to_string(),
                    VpnTransportProtocol::Udp => "udp".to_string(),
                },
            );
            if !openvpn.ca_cert.trim().is_empty() {
                data.insert("ca".to_string(), openvpn.ca_cert.clone());
            }
            if !openvpn.client_cert.trim().is_empty() {
                data.insert("cert".to_string(), openvpn.client_cert.clone());
            }
            if !openvpn.private_key.trim().is_empty() {
                data.insert("key".to_string(), openvpn.private_key.clone());
            }
            vpn.insert("data".into(), owned_value(data));

            if !openvpn.password.trim().is_empty() {
                vpn.insert(
                    "secrets".into(),
                    owned_value(HashMap::from([(
                        "password".to_string(),
                        openvpn.password.clone(),
                    )])),
                );
            }

            settings.insert("vpn".into(), vpn);
        }
        VpnConfigKind::WireGuard(wireguard) => {
            let mut wg = HashMap::new();
            if !wireguard.private_key.trim().is_empty() {
                wg.insert(
                    "private-key".into(),
                    owned_value(wireguard.private_key.clone()),
                );
            }
            if let Some(listen_port) = wireguard.listen_port {
                wg.insert("listen-port".into(), owned_value(u32::from(listen_port)));
            }
            if let Some(mtu) = wireguard.mtu {
                wg.insert("mtu".into(), owned_value(mtu));
            }

            let peers = wireguard
                .peers
                .iter()
                .map(|peer| {
                    let mut map: HashMap<String, OwnedValue> = HashMap::new();
                    map.insert("public-key".into(), owned_value(peer.public_key.clone()));
                    if !peer.preshared_key.trim().is_empty() {
                        map.insert(
                            "preshared-key".into(),
                            owned_value(peer.preshared_key.clone()),
                        );
                    }
                    if !peer.endpoint.trim().is_empty() {
                        map.insert("endpoint".into(), owned_value(peer.endpoint.clone()));
                    }
                    if !peer.allowed_ips.is_empty() {
                        map.insert("allowed-ips".into(), owned_value(peer.allowed_ips.clone()));
                    }
                    if let Some(keepalive) = peer.persistent_keepalive {
                        map.insert(
                            "persistent-keepalive".into(),
                            owned_value(u32::from(keepalive)),
                        );
                    }
                    map
                })
                .collect::<Vec<_>>();
            wg.insert("peers".into(), owned_value(peers));
            settings.insert("wireguard".into(), wg);
        }
    }

    settings
}

fn is_hotspot_profile(
    settings: &HashMap<String, HashMap<String, OwnedValue>>,
    interface_name: &str,
) -> bool {
    let connection = settings.get("connection");
    let connection_type = connection
        .and_then(|section| section.get("type"))
        .and_then(owned_value_to_string)
        .unwrap_or_default();
    if connection_type != "802-11-wireless" {
        return false;
    }

    let profile_interface = connection
        .and_then(|section| section.get("interface-name"))
        .and_then(owned_value_to_string)
        .unwrap_or_default();
    if !profile_interface.is_empty() && profile_interface != interface_name {
        return false;
    }

    settings
        .get("802-11-wireless")
        .and_then(|section| section.get("mode"))
        .and_then(owned_value_to_string)
        .as_deref()
        == Some("ap")
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

fn owned_value_to_bool(value: &OwnedValue) -> Option<bool> {
    bool::try_from(value.clone()).ok()
}

fn owned_value_to_u16(value: &OwnedValue) -> Option<u16> {
    u16::try_from(value.clone()).ok().or_else(|| {
        u32::try_from(value.clone())
            .ok()
            .and_then(|value| u16::try_from(value).ok())
    })
}

fn owned_value_to_u32(value: &OwnedValue, allow_i32: bool) -> Option<u32> {
    u32::try_from(value.clone()).ok().or_else(|| {
        allow_i32
            .then(|| i32::try_from(value.clone()).ok())
            .flatten()
            .and_then(|value| u32::try_from(value).ok())
    })
}

fn owned_value_to_string_vec(value: &OwnedValue) -> Option<Vec<String>> {
    Vec::<String>::try_from(value.clone()).ok()
}

fn owned_value_to_string_map(value: &OwnedValue) -> Option<HashMap<String, String>> {
    HashMap::<String, String>::try_from(value.clone()).ok()
}

fn owned_value_to_address_data(value: &OwnedValue) -> Option<Vec<HashMap<String, OwnedValue>>> {
    Vec::<HashMap<String, OwnedValue>>::try_from(value.clone()).ok()
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

    #[test]
    fn parse_openvpn_profile_extracts_typed_fields() {
        let mut connection = HashMap::new();
        connection.insert("id".into(), owned_value("Work VPN"));
        connection.insert("uuid".into(), owned_value("vpn-1"));
        connection.insert("type".into(), owned_value("vpn"));
        connection.insert("autoconnect".into(), owned_value(true));

        let mut vpn = HashMap::new();
        vpn.insert(
            "service-type".into(),
            owned_value("org.freedesktop.NetworkManager.openvpn"),
        );
        vpn.insert("user-name".into(), owned_value("alice"));
        vpn.insert(
            "data".into(),
            owned_value(HashMap::from([
                ("remote".to_string(), "vpn.example.com".to_string()),
                ("port".to_string(), "1194".to_string()),
                ("proto".to_string(), "udp".to_string()),
                ("ca".to_string(), "/etc/openvpn/ca.crt".to_string()),
            ])),
        );
        vpn.insert(
            "secrets".into(),
            owned_value(HashMap::from([(
                "password".to_string(),
                "secret".to_string(),
            )])),
        );

        let settings = HashMap::from([("connection".into(), connection), ("vpn".into(), vpn)]);
        let config = parse_vpn_profile_config(&settings, "/settings/vpn-1");

        assert_eq!(config.id, "Work VPN");
        assert_eq!(config.settings_path.as_deref(), Some("/settings/vpn-1"));
        assert_eq!(config.kind.connection_type(), "vpn");
        match config.kind {
            VpnConfigKind::OpenVpn(openvpn) => {
                assert_eq!(openvpn.gateway, "vpn.example.com");
                assert_eq!(openvpn.port, Some(1194));
                assert_eq!(openvpn.protocol, VpnTransportProtocol::Udp);
                assert_eq!(openvpn.username, "alice");
                assert_eq!(openvpn.password, "secret");
                assert_eq!(openvpn.ca_cert, "/etc/openvpn/ca.crt");
            }
            other => panic!("expected openvpn config, got {other:?}"),
        }
    }

    #[test]
    fn vpn_profile_settings_map_preserves_wireguard_fields() {
        let config = VpnProfileConfig {
            id: "Studio Tunnel".into(),
            uuid: Some("wg-1".into()),
            settings_path: Some("/settings/wg-1".into()),
            autoconnect: false,
            interface_name: Some("wg0".into()),
            ipv4: NetworkIpConfig {
                method: NetworkIpMethod::Manual,
                address: "10.20.0.2".into(),
                prefix: Some(24),
                gateway: "10.20.0.1".into(),
                dns: vec!["1.1.1.1".into()],
            },
            ipv6: NetworkIpConfig::default(),
            kind: VpnConfigKind::WireGuard(WireGuardConfig {
                private_key: "private-key".into(),
                listen_port: Some(51820),
                mtu: Some(1420),
                peers: vec![WireGuardPeerConfig {
                    public_key: "public-key".into(),
                    preshared_key: "psk".into(),
                    endpoint: "wg.example.com:51820".into(),
                    allowed_ips: vec!["0.0.0.0/0".into(), "::/0".into()],
                    persistent_keepalive: Some(25),
                }],
            }),
        };

        let settings = vpn_profile_settings_map(&config);
        let reparsed = parse_vpn_profile_config(&settings, "/settings/wg-1");

        assert_eq!(reparsed.id, "Studio Tunnel");
        assert_eq!(reparsed.interface_name.as_deref(), Some("wg0"));
        match reparsed.kind {
            VpnConfigKind::WireGuard(wireguard) => {
                assert_eq!(wireguard.private_key, "private-key");
                assert_eq!(wireguard.listen_port, Some(51820));
                assert_eq!(wireguard.mtu, Some(1420));
                assert_eq!(wireguard.peers.len(), 1);
                let peer = &wireguard.peers[0];
                assert_eq!(peer.public_key, "public-key");
                assert_eq!(peer.preshared_key, "psk");
                assert_eq!(peer.endpoint, "wg.example.com:51820");
                assert_eq!(peer.allowed_ips, vec!["0.0.0.0/0", "::/0"]);
                assert_eq!(peer.persistent_keepalive, Some(25));
            }
            other => panic!("expected wireguard config, got {other:?}"),
        }
    }
}
