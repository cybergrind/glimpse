use std::collections::HashMap;
use std::pin::Pin;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};
use crate::providers::dbus_props::DbusPropertyGroup;

const NAME: &str = "network";
const TOPICS: &[&str] = &[
    "network.status",
    "network.wifi",
    "network.connections",
    "network.devices",
    "network.saved_vpns",
];
const METHODS: &[&str] = &[
    "network.set_wifi_enabled",
    "network.set_enabled",
    "network.wifi_scan",
    "network.connect",
    "network.connect_uuid",
    "network.disconnect",
    "network.forget",
];

const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const NM_IFACE: &str = "org.freedesktop.NetworkManager";
const NM_SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const NM_SETTINGS_IFACE: &str = "org.freedesktop.NetworkManager.Settings";

#[derive(Debug, Clone, Serialize, Default)]
struct NetworkStatus {
    connectivity: String,
    enabled: bool,
    wifi_enabled: bool,
    wifi_hw_enabled: bool,
    primary_connection: String,
    primary_type: String,
    metered: bool,
    speed: u32,
    icon: String,
}

#[derive(Debug, Clone, Serialize)]
struct WifiAccessPoint {
    ssid: String,
    strength: u8,
    frequency: u32,
    security: String,
    connected: bool,
    saved: bool,
    uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct NetworkConnection {
    id: String,
    uuid: String,
    connection_type: String,
    device: String,
    state: String,
    vpn: bool,
    ip4_address: Option<String>,
    gateway: Option<String>,
    dns: Vec<String>,
    speed: u32,
}

#[derive(Debug, Clone, Serialize)]
struct NetworkDevice {
    interface: String,
    device_type: String,
    state: String,
    speed: u32,
    carrier: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct SavedVpn {
    id: String,
    uuid: String,
    connection_type: String,
    active: bool,
    state: Option<String>,
}

fn connectivity_str(v: u32) -> &'static str {
    match v {
        1 => "none",
        2 => "portal",
        3 => "limited",
        4 => "full",
        _ => "unknown",
    }
}

fn device_state_str(v: u32) -> &'static str {
    match v {
        100 => "connected",
        20 => "unavailable",
        110 => "deactivating",
        30..=90 => "connecting",
        _ => "disconnected",
    }
}

fn device_type_str(v: u32) -> &'static str {
    match v {
        1 => "ethernet",
        2 => "wifi",
        29 => "wireguard",
        _ => "other",
    }
}

fn connection_state_str(v: u32) -> &'static str {
    match v {
        1 => "activating",
        2 => "activated",
        3 => "deactivating",
        _ => "unknown",
    }
}

fn connection_type_str(s: &str) -> &'static str {
    match s {
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

struct NetworkProvider {
    status: NetworkStatus,
    access_points: Vec<WifiAccessPoint>,
    connections: Vec<NetworkConnection>,
    devices: Vec<NetworkDevice>,
    saved_vpns: Vec<SavedVpn>,
}

impl Provider for NetworkProvider {
    fn name(&self) -> &'static str {
        NAME
    }
    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }
    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }

    fn run(
        &mut self,
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            tracing::info!("network: starting");
            let conn = zbus::Connection::system().await?;

            self.full_scan(&conn).await;
            tracing::info!(
                connectivity = %self.status.connectivity,
                devices = self.devices.len(),
                connections = self.connections.len(),
                aps = self.access_points.len(),
                "network: initial scan"
            );
            self.emit_all(&events).await;

            // Trigger a WiFi scan so NM refreshes its stale AP cache.
            // Results arrive via D-Bus signals and trigger a rescan.
            if self.status.wifi_enabled {
                let _ = self.wifi_scan(&conn).await;
            }

            // Monitor NM manager PropertiesChanged (connectivity, enabled, active connections)
            let nm_props_proxy = zbus::fdo::PropertiesProxy::builder(&conn)
                .destination(NM_SERVICE)?
                .path(zbus::zvariant::ObjectPath::try_from(NM_PATH)?)?
                .build()
                .await?;
            let mut nm_changes = nm_props_proxy.receive_properties_changed().await?;

            // Monitor WiFi device PropertiesChanged via per-device PropertiesProxy
            let mut wifi_prop_streams = Vec::new();
            let device_paths: Vec<zbus::zvariant::OwnedObjectPath> = {
                let nm_tmp = DbusPropertyGroup::new(&conn, NM_SERVICE, NM_PATH, NM_IFACE).await?;
                nm_tmp.call("GetDevices", &()).await.unwrap_or_default()
            };
            for dev_path in &device_paths {
                let dev_str = dev_path.to_string();
                if let Ok(dev) = DbusPropertyGroup::new(&conn, NM_SERVICE, &dev_str, "org.freedesktop.NetworkManager.Device").await {
                    if dev.get::<u32>("DeviceType").await.unwrap_or(0) == 2 {
                        // Watch PropertiesChanged on the Wireless interface directly
                        if let Ok(props) = zbus::fdo::PropertiesProxy::builder(&conn)
                            .destination(NM_SERVICE)?
                            .path(zbus::zvariant::ObjectPath::try_from(dev_str.as_str())?)?
                            .build()
                            .await
                        {
                            if let Ok(s) = props.receive_properties_changed().await {
                                wifi_prop_streams.push(s);
                            }
                        }
                    }
                }
            }
            let mut wifi_stream = futures_util::stream::select_all(wifi_prop_streams);

            let mut dirty = false;
            let debounce = tokio::time::sleep(std::time::Duration::from_secs(86400));
            tokio::pin!(debounce);

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req, &conn).await;
                    }
                    Some(_changed) = nm_changes.next() => {
                        tracing::debug!("network: NM manager properties changed");
                        dirty = true;
                        debounce.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(500));
                    }
                    Some(_changed) = wifi_stream.next() => {
                        tracing::debug!("network: wifi device properties changed");
                        dirty = true;
                        debounce.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(500));
                    }
                    () = &mut debounce, if dirty => {
                        dirty = false;
                        self.full_scan(&conn).await;
                        tracing::info!(
                            aps = self.access_points.len(),
                            connections = self.connections.len(),
                            "network: rescan"
                        );
                        self.emit_all(&events).await;
                    }
                }
            }
            Ok(())
        })
    }
}


impl NetworkProvider {
    async fn full_scan(&mut self, conn: &zbus::Connection) {
        self.access_points.clear();
        self.connections.clear();
        self.devices.clear();
        self.saved_vpns.clear();
        self.status = NetworkStatus::default();

        self.scan_status(conn).await;
        self.scan_devices(conn).await;
        self.scan_connections(conn).await;
        self.scan_access_points(conn).await;
        self.scan_saved_vpns(conn).await;
        self.resolve_icon();
    }

    async fn scan_status(&mut self, conn: &zbus::Connection) {
        let Ok(nm) = DbusPropertyGroup::new(conn, NM_SERVICE, NM_PATH, NM_IFACE).await else {
            return;
        };

        self.status.connectivity =
            connectivity_str(nm.get::<u32>("Connectivity").await.unwrap_or(0)).into();
        self.status.enabled = nm.get::<bool>("NetworkingEnabled").await.unwrap_or(false);
        self.status.wifi_enabled = nm.get::<bool>("WirelessEnabled").await.unwrap_or(false);
        self.status.wifi_hw_enabled =
            nm.get::<bool>("WirelessHardwareEnabled").await.unwrap_or(false);
        self.status.metered = matches!(nm.get::<u32>("Metered").await.unwrap_or(0), 1 | 3);

        let primary_path: String = nm
            .get::<zbus::zvariant::OwnedObjectPath>("PrimaryConnection")
            .await
            .map(|p| p.to_string())
            .unwrap_or_default();

        if !primary_path.is_empty() && primary_path != "/" {
            if let Ok(active) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &primary_path,
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await
            {
                self.status.primary_connection =
                    active.get::<String>("Id").await.unwrap_or_default();
                let raw_type = active.get::<String>("Type").await.unwrap_or_default();
                self.status.primary_type = connection_type_str(&raw_type).into();
            }
        }
    }

    async fn scan_devices(&mut self, conn: &zbus::Connection) {
        let Ok(nm) = DbusPropertyGroup::new(conn, NM_SERVICE, NM_PATH, NM_IFACE).await else {
            return;
        };

        let device_paths: Vec<zbus::zvariant::OwnedObjectPath> =
            match nm.call("GetDevices", &()).await {
                Ok(p) => p,
                Err(_) => return,
            };

        for path in &device_paths {
            let path_str = path.to_string();
            let Ok(dev) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &path_str,
                "org.freedesktop.NetworkManager.Device",
            )
            .await
            else {
                continue;
            };

            let dev_type_num = dev.get::<u32>("DeviceType").await.unwrap_or(0);
            let dev_type = device_type_str(dev_type_num);
            if dev_type == "other" {
                continue;
            }

            let state_num = dev.get::<u32>("State").await.unwrap_or(0);
            let interface = dev.get::<String>("Interface").await.unwrap_or_default();

            let mut speed = 0u32;
            let mut carrier = None;

            if dev_type == "ethernet" {
                if let Ok(wired) = DbusPropertyGroup::new(
                    conn,
                    NM_SERVICE,
                    &path_str,
                    "org.freedesktop.NetworkManager.Device.Wired",
                )
                .await
                {
                    carrier = wired.get::<bool>("Carrier").await;
                    speed = wired.get::<u32>("Speed").await.unwrap_or(0);
                }
            } else if dev_type == "wifi" {
                speed = dev.get::<u32>("Speed").await.unwrap_or(0);
            }

            if dev_type == "ethernet" && state_num == 100 {
                self.status.speed = speed;
            }

            self.devices.push(NetworkDevice {
                interface,
                device_type: dev_type.into(),
                state: device_state_str(state_num).into(),
                speed,
                carrier,
            });
        }
    }

    async fn scan_connections(&mut self, conn: &zbus::Connection) {
        let Ok(nm) = DbusPropertyGroup::new(conn, NM_SERVICE, NM_PATH, NM_IFACE).await else {
            return;
        };

        let active_paths: Vec<zbus::zvariant::OwnedObjectPath> = nm
            .get::<Vec<zbus::zvariant::OwnedObjectPath>>("ActiveConnections")
            .await
            .unwrap_or_default();

        for path in &active_paths {
            let path_str = path.to_string();
            let Ok(active) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &path_str,
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await
            else {
                continue;
            };

            let id = active.get::<String>("Id").await.unwrap_or_default();
            let uuid = active.get::<String>("Uuid").await.unwrap_or_default();
            let raw_type = active.get::<String>("Type").await.unwrap_or_default();
            let state_num = active.get::<u32>("State").await.unwrap_or(0);
            let vpn = active.get::<bool>("Vpn").await.unwrap_or(false);

            let device_paths: Vec<zbus::zvariant::OwnedObjectPath> = active
                .get::<Vec<zbus::zvariant::OwnedObjectPath>>("Devices")
                .await
                .unwrap_or_default();
            let mut device_name = String::new();
            let mut speed = 0u32;
            if let Some(dev_path) = device_paths.first() {
                let dev_path_str = dev_path.to_string();
                if let Ok(dev) = DbusPropertyGroup::new(
                    conn,
                    NM_SERVICE,
                    &dev_path_str,
                    "org.freedesktop.NetworkManager.Device",
                )
                .await
                {
                    device_name = dev.get::<String>("Interface").await.unwrap_or_default();
                    speed = dev.get::<u32>("Speed").await.unwrap_or(0);
                }
            }

            let mut ip4_address = None;
            let mut gateway = None;
            let mut dns = Vec::new();

            let ip4_path = active
                .get::<zbus::zvariant::OwnedObjectPath>("Ip4Config")
                .await
                .map(|p| p.to_string())
                .unwrap_or_default();
            if !ip4_path.is_empty() && ip4_path != "/" {
                if let Ok(ip4) = DbusPropertyGroup::new(
                    conn,
                    NM_SERVICE,
                    &ip4_path,
                    "org.freedesktop.NetworkManager.IP4Config",
                )
                .await
                {
                    gateway = ip4.get::<String>("Gateway").await;

                    if let Some(addr_data) = ip4
                        .get::<Vec<HashMap<String, zbus::zvariant::OwnedValue>>>("AddressData")
                        .await
                    {
                        if let Some(first) = addr_data.first() {
                            if let Some(addr_val) = first.get("address") {
                                ip4_address = String::try_from(addr_val.clone()).ok();
                            }
                        }
                    }

                    if let Some(ns_data) = ip4
                        .get::<Vec<HashMap<String, zbus::zvariant::OwnedValue>>>("NameserverData")
                        .await
                    {
                        for entry in &ns_data {
                            if let Some(addr_val) = entry.get("address") {
                                if let Ok(addr) = String::try_from(addr_val.clone()) {
                                    dns.push(addr);
                                }
                            }
                        }
                    }
                }
            }

            let conn_type = connection_type_str(&raw_type);
            if conn_type == "wifi" && state_num == 2 {
                self.status.speed = speed;
            }

            self.connections.push(NetworkConnection {
                id,
                uuid,
                connection_type: conn_type.into(),
                device: device_name,
                state: connection_state_str(state_num).into(),
                vpn,
                ip4_address,
                gateway,
                dns,
                speed,
            });
        }
    }

    async fn scan_access_points(&mut self, conn: &zbus::Connection) {
        if !self.status.wifi_enabled {
            return;
        }

        let connected_ssids: HashMap<String, String> = self
            .connections
            .iter()
            .filter(|c| c.connection_type == "wifi" && c.state == "activated")
            .map(|c| (c.id.clone(), c.uuid.clone()))
            .collect();

        let saved_wifi = self.get_saved_wifi(conn).await;

        let Ok(nm) = DbusPropertyGroup::new(conn, NM_SERVICE, NM_PATH, NM_IFACE).await else {
            return;
        };

        let device_paths: Vec<zbus::zvariant::OwnedObjectPath> =
            match nm.call("GetDevices", &()).await {
                Ok(p) => p,
                Err(_) => return,
            };

        let mut best_aps: HashMap<String, WifiAccessPoint> = HashMap::new();

        for dev_path in &device_paths {
            let dev_path_str = dev_path.to_string();
            let Ok(dev) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &dev_path_str,
                "org.freedesktop.NetworkManager.Device",
            )
            .await
            else {
                continue;
            };

            let dev_type = dev.get::<u32>("DeviceType").await.unwrap_or(0);
            if dev_type != 2 {
                continue;
            }

            let Ok(wifi) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &dev_path_str,
                "org.freedesktop.NetworkManager.Device.Wireless",
            )
            .await
            else {
                continue;
            };

            let ap_paths: Vec<zbus::zvariant::OwnedObjectPath> =
                match wifi.call("GetAllAccessPoints", &()).await {
                    Ok(p) => p,
                    Err(_) => continue,
                };

            for ap_path in &ap_paths {
                let ap_path_str = ap_path.to_string();
                let Ok(ap) = DbusPropertyGroup::new(
                    conn,
                    NM_SERVICE,
                    &ap_path_str,
                    "org.freedesktop.NetworkManager.AccessPoint",
                )
                .await
                else {
                    continue;
                };

                let ssid_bytes: Vec<u8> = ap.get::<Vec<u8>>("Ssid").await.unwrap_or_default();
                let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();
                if ssid.is_empty() {
                    continue;
                }

                let strength = ap.get::<u8>("Strength").await.unwrap_or(0);
                let frequency = ap.get::<u32>("Frequency").await.unwrap_or(0);
                let flags = ap.get::<u32>("Flags").await.unwrap_or(0);
                let wpa_flags = ap.get::<u32>("WpaFlags").await.unwrap_or(0);
                let rsn_flags = ap.get::<u32>("RsnFlags").await.unwrap_or(0);

                let connected = connected_ssids.contains_key(&ssid);
                let saved_uuid = saved_wifi.get(&ssid).cloned();
                let saved = saved_uuid.is_some() || connected;

                let uuid = if connected {
                    connected_ssids.get(&ssid).cloned()
                } else {
                    saved_uuid
                };

                let entry = best_aps.entry(ssid.clone()).or_insert(WifiAccessPoint {
                    ssid,
                    strength,
                    frequency,
                    security: ap_security(flags, wpa_flags, rsn_flags).into(),
                    connected,
                    saved,
                    uuid,
                });

                if strength > entry.strength {
                    entry.strength = strength;
                    entry.frequency = frequency;
                }
            }
        }

        self.access_points = best_aps.into_values().collect();
        self.access_points.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then(b.strength.cmp(&a.strength))
        });
    }

    async fn get_saved_wifi(
        &self,
        conn: &zbus::Connection,
    ) -> HashMap<String, String> {
        let mut saved: HashMap<String, String> = HashMap::new();
        let Ok(settings) =
            DbusPropertyGroup::new(conn, NM_SERVICE, NM_SETTINGS_PATH, NM_SETTINGS_IFACE).await
        else {
            return saved;
        };

        let conn_paths: Vec<zbus::zvariant::OwnedObjectPath> =
            match settings.call("ListConnections", &()).await {
                Ok(p) => p,
                Err(_) => return saved,
            };

        for path in &conn_paths {
            let path_str = path.to_string();
            let Ok(sc) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &path_str,
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await
            else {
                continue;
            };

            let settings_map: HashMap<
                String,
                HashMap<String, zbus::zvariant::OwnedValue>,
            > = match sc.call("GetSettings", &()).await {
                Ok(s) => s,
                Err(_) => continue,
            };

            let Some(conn_section) = settings_map.get("connection") else {
                continue;
            };
            let conn_type = conn_section
                .get("type")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();
            if conn_type != "802-11-wireless" {
                continue;
            }

            let uuid = conn_section
                .get("uuid")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();

            if let Some(wifi_section) = settings_map.get("802-11-wireless") {
                if let Some(ssid_val) = wifi_section.get("ssid") {
                    if let Ok(ssid_bytes) = <Vec<u8>>::try_from(ssid_val.clone()) {
                        let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();
                        if !ssid.is_empty() && !uuid.is_empty() {
                            saved.insert(ssid, uuid);
                        }
                    }
                }
            }
        }

        saved
    }

    async fn scan_saved_vpns(&mut self, conn: &zbus::Connection) {
        let Ok(settings) =
            DbusPropertyGroup::new(conn, NM_SERVICE, NM_SETTINGS_PATH, NM_SETTINGS_IFACE).await
        else {
            return;
        };

        let conn_paths: Vec<zbus::zvariant::OwnedObjectPath> =
            match settings.call("ListConnections", &()).await {
                Ok(p) => p,
                Err(_) => return,
            };

        let active_vpns: HashMap<String, String> = self
            .connections
            .iter()
            .filter(|c| c.vpn || c.connection_type == "vpn" || c.connection_type == "wireguard")
            .map(|c| (c.uuid.clone(), c.state.clone()))
            .collect();

        for path in &conn_paths {
            let path_str = path.to_string();
            let Ok(sc) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &path_str,
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await
            else {
                continue;
            };

            let settings_map: HashMap<
                String,
                HashMap<String, zbus::zvariant::OwnedValue>,
            > = match sc.call("GetSettings", &()).await {
                Ok(s) => s,
                Err(_) => continue,
            };

            let Some(conn_section) = settings_map.get("connection") else {
                continue;
            };
            let conn_type = conn_section
                .get("type")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();
            if conn_type != "vpn" && conn_type != "wireguard" {
                continue;
            }

            let id = conn_section
                .get("id")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();
            let uuid = conn_section
                .get("uuid")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();

            let active_state = active_vpns.get(&uuid);

            self.saved_vpns.push(SavedVpn {
                id,
                uuid,
                connection_type: connection_type_str(&conn_type).into(),
                active: active_state.is_some(),
                state: active_state.cloned(),
            });
        }
    }

    fn resolve_icon(&mut self) {
        if !self.status.enabled {
            self.status.icon = "network-offline-symbolic".into();
            return;
        }

        let has_wifi_connected = self
            .connections
            .iter()
            .any(|c| c.connection_type == "wifi" && c.state == "activated");

        if has_wifi_connected {
            let strength = self
                .access_points
                .iter()
                .find(|ap| ap.connected)
                .map(|ap| ap.strength)
                .unwrap_or(0);
            self.status.icon = wifi_icon(strength).into();
            return;
        }

        let has_wired_connected = self
            .connections
            .iter()
            .any(|c| c.connection_type == "ethernet" && c.state == "activated");

        if has_wired_connected {
            self.status.icon = "network-wired-symbolic".into();
            return;
        }

        if !self.status.wifi_enabled {
            self.status.icon = "network-wireless-disabled-symbolic".into();
            return;
        }

        self.status.icon = "network-offline-symbolic".into();
    }

    async fn emit_all(&self, events: &mpsc::Sender<ProviderEvent>) {
        let _ = events
            .send(ProviderEvent {
                topic: "network.status".into(),
                data: serde_json::to_value(&self.status).unwrap_or_default(),
            })
            .await;
        let _ = events
            .send(ProviderEvent {
                topic: "network.wifi".into(),
                data: serde_json::to_value(&self.access_points).unwrap_or_default(),
            })
            .await;
        let _ = events
            .send(ProviderEvent {
                topic: "network.connections".into(),
                data: serde_json::to_value(&self.connections).unwrap_or_default(),
            })
            .await;
        let _ = events
            .send(ProviderEvent {
                topic: "network.devices".into(),
                data: serde_json::to_value(&self.devices).unwrap_or_default(),
            })
            .await;
        let _ = events
            .send(ProviderEvent {
                topic: "network.saved_vpns".into(),
                data: serde_json::to_value(&self.saved_vpns).unwrap_or_default(),
            })
            .await;
    }

    async fn handle_request(&mut self, req: ProviderRequest, conn: &zbus::Connection) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "network.status" => serde_json::to_value(&self.status).ok(),
                    "network.wifi" => serde_json::to_value(&self.access_points).ok(),
                    "network.connections" => serde_json::to_value(&self.connections).ok(),
                    "network.devices" => serde_json::to_value(&self.devices).ok(),
                    "network.saved_vpns" => serde_json::to_value(&self.saved_vpns).ok(),
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
                    "network.set_wifi_enabled" => {
                        let Some(enabled) = params["enabled"].as_bool() else {
                            let _ =
                                reply.send(Err(anyhow::anyhow!("missing 'enabled' (bool)")));
                            return;
                        };
                        tracing::info!("setting wifi enabled to {enabled}");
                        self.set_wifi_enabled(conn, enabled).await
                    }
                    "network.set_enabled" => {
                        let Some(enabled) = params["enabled"].as_bool() else {
                            let _ =
                                reply.send(Err(anyhow::anyhow!("missing 'enabled' (bool)")));
                            return;
                        };
                        tracing::info!("setting networking enabled to {enabled}");
                        self.set_enabled(conn, enabled).await
                    }
                    "network.wifi_scan" => {
                        tracing::info!("requesting wifi scan");
                        self.wifi_scan(conn).await
                    }
                    "network.connect" => {
                        let Some(ssid) = params["ssid"].as_str() else {
                            let _ = reply.send(Err(anyhow::anyhow!("missing 'ssid' (string)")));
                            return;
                        };
                        let password = params["password"].as_str().map(|s| s.to_owned());
                        let has_pw = password.is_some();
                        tracing::info!("connecting to \"{ssid}\" (password: {has_pw})");
                        self.connect(conn, ssid, password.as_deref()).await
                    }
                    "network.connect_uuid" => {
                        let Some(uuid) = params["uuid"].as_str() else {
                            let _ = reply.send(Err(anyhow::anyhow!("missing 'uuid' (string)")));
                            return;
                        };
                        let name = self.connections.iter()
                            .find(|c| c.uuid == uuid)
                            .map(|c| c.id.clone())
                            .or_else(|| self.saved_vpns.iter().find(|v| v.uuid == uuid).map(|v| v.id.clone()))
                            .unwrap_or_else(|| uuid.to_string());
                        tracing::info!("activating saved connection \"{name}\"");
                        self.connect_uuid(conn, uuid).await
                    }
                    "network.disconnect" => {
                        let Some(uuid) = params["uuid"].as_str() else {
                            let _ = reply.send(Err(anyhow::anyhow!("missing 'uuid' (string)")));
                            return;
                        };
                        let name = self.connections.iter()
                            .find(|c| c.uuid == uuid)
                            .map(|c| c.id.clone())
                            .unwrap_or_else(|| uuid.to_string());
                        tracing::info!("disconnecting \"{name}\"");
                        self.disconnect(conn, uuid).await
                    }
                    "network.forget" => {
                        let Some(uuid) = params["uuid"].as_str() else {
                            let _ = reply.send(Err(anyhow::anyhow!("missing 'uuid' (string)")));
                            return;
                        };
                        tracing::info!("forgetting connection {uuid}");
                        self.forget(conn, uuid).await
                    }
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                match &result {
                    Ok(_) => tracing::info!(method = %method, "network: call succeeded"),
                    Err(e) => tracing::warn!(method = %method, error = %e, "network: call failed"),
                }
                let _ = reply.send(result);
            }
        }
    }

    async fn set_wifi_enabled(
        &self,
        conn: &zbus::Connection,
        enabled: bool,
    ) -> anyhow::Result<serde_json::Value> {
        let nm = DbusPropertyGroup::new(conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        nm.set("WirelessEnabled", enabled)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(json!(null))
    }

    async fn set_enabled(
        &self,
        conn: &zbus::Connection,
        enabled: bool,
    ) -> anyhow::Result<serde_json::Value> {
        let nm = DbusPropertyGroup::new(conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        nm.call_void("Enable", &(enabled,))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(json!(null))
    }

    async fn wifi_scan(&self, conn: &zbus::Connection) -> anyhow::Result<serde_json::Value> {
        let nm = DbusPropertyGroup::new(conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let device_paths: Vec<zbus::zvariant::OwnedObjectPath> = nm
            .call("GetDevices", &())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let empty_opts: HashMap<String, zbus::zvariant::Value<'_>> = HashMap::new();

        for dev_path in &device_paths {
            let dev_path_str = dev_path.to_string();
            let Ok(dev) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &dev_path_str,
                "org.freedesktop.NetworkManager.Device",
            )
            .await
            else {
                continue;
            };

            let dev_type = dev.get::<u32>("DeviceType").await.unwrap_or(0);
            if dev_type != 2 {
                continue;
            }

            let Ok(wifi) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &dev_path_str,
                "org.freedesktop.NetworkManager.Device.Wireless",
            )
            .await
            else {
                continue;
            };

            let _ = wifi.call_void("RequestScan", &(&empty_opts,)).await;
        }

        Ok(json!(null))
    }

    async fn connect(
        &self,
        conn: &zbus::Connection,
        ssid: &str,
        password: Option<&str>,
    ) -> anyhow::Result<serde_json::Value> {
        let nm = DbusPropertyGroup::new(conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let device_paths: Vec<zbus::zvariant::OwnedObjectPath> = nm
            .call("GetDevices", &())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut wifi_dev_path: Option<String> = None;
        let mut target_ap_path: Option<String> = None;

        for dev_path in &device_paths {
            let dev_path_str = dev_path.to_string();
            let Ok(dev) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &dev_path_str,
                "org.freedesktop.NetworkManager.Device",
            )
            .await
            else {
                continue;
            };

            let dev_type = dev.get::<u32>("DeviceType").await.unwrap_or(0);
            if dev_type != 2 {
                continue;
            }

            let Ok(wifi) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &dev_path_str,
                "org.freedesktop.NetworkManager.Device.Wireless",
            )
            .await
            else {
                continue;
            };

            let ap_paths: Vec<zbus::zvariant::OwnedObjectPath> =
                match wifi.call("GetAllAccessPoints", &()).await {
                    Ok(p) => p,
                    Err(_) => continue,
                };

            for ap_path in &ap_paths {
                let ap_path_str = ap_path.to_string();
                let Ok(ap) = DbusPropertyGroup::new(
                    conn,
                    NM_SERVICE,
                    &ap_path_str,
                    "org.freedesktop.NetworkManager.AccessPoint",
                )
                .await
                else {
                    continue;
                };

                let ssid_bytes: Vec<u8> = ap.get::<Vec<u8>>("Ssid").await.unwrap_or_default();
                let ap_ssid = String::from_utf8_lossy(&ssid_bytes);
                if ap_ssid == ssid {
                    wifi_dev_path = Some(dev_path_str.clone());
                    target_ap_path = Some(ap_path_str);
                    break;
                }
            }

            if target_ap_path.is_some() {
                break;
            }
        }

        let wifi_dev = wifi_dev_path.ok_or_else(|| anyhow::anyhow!("no wifi device found"))?;
        let ap = target_ap_path.ok_or_else(|| anyhow::anyhow!("access point not found: {ssid}"))?;

        let dev_obj = zbus::zvariant::ObjectPath::try_from(wifi_dev)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let ap_obj =
            zbus::zvariant::ObjectPath::try_from(ap).map_err(|e| anyhow::anyhow!("{e}"))?;

        let mut settings: HashMap<String, HashMap<String, zbus::zvariant::Value<'_>>> =
            HashMap::new();

        let mut conn_settings: HashMap<String, zbus::zvariant::Value<'_>> = HashMap::new();
        conn_settings.insert("type".into(), "802-11-wireless".into());
        settings.insert("connection".into(), conn_settings);

        let mut wifi_settings: HashMap<String, zbus::zvariant::Value<'_>> = HashMap::new();
        wifi_settings.insert("ssid".into(), zbus::zvariant::Value::from(ssid.as_bytes().to_vec()));
        settings.insert("802-11-wireless".into(), wifi_settings);

        if let Some(pw) = password {
            let mut sec_settings: HashMap<String, zbus::zvariant::Value<'_>> = HashMap::new();
            sec_settings.insert("key-mgmt".into(), "wpa-psk".into());
            sec_settings.insert("psk".into(), pw.into());
            settings.insert("802-11-wireless-security".into(), sec_settings);

            if let Some(ws) = settings.get_mut("802-11-wireless") {
                ws.insert("security".into(), "802-11-wireless-security".into());
            }
        }

        let _: (
            zbus::zvariant::OwnedObjectPath,
            zbus::zvariant::OwnedObjectPath,
        ) = nm
            .call(
                "AddAndActivateConnection",
                &(&settings, &dev_obj, &ap_obj),
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        Ok(json!(null))
    }

    async fn connect_uuid(
        &self,
        conn: &zbus::Connection,
        uuid: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let settings = DbusPropertyGroup::new(conn, NM_SERVICE, NM_SETTINGS_PATH, NM_SETTINGS_IFACE)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let conn_path: zbus::zvariant::OwnedObjectPath = settings
            .call("GetConnectionByUuid", &(uuid,))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let nm = DbusPropertyGroup::new(conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let empty_dev =
            zbus::zvariant::ObjectPath::try_from("/").map_err(|e| anyhow::anyhow!("{e}"))?;
        let empty_specific =
            zbus::zvariant::ObjectPath::try_from("/").map_err(|e| anyhow::anyhow!("{e}"))?;
        let conn_obj = zbus::zvariant::ObjectPath::try_from(conn_path.as_str())
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let _: zbus::zvariant::OwnedObjectPath = nm
            .call(
                "ActivateConnection",
                &(&conn_obj, &empty_dev, &empty_specific),
            )
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        Ok(json!(null))
    }

    async fn disconnect(
        &self,
        conn: &zbus::Connection,
        uuid: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let nm = DbusPropertyGroup::new(conn, NM_SERVICE, NM_PATH, NM_IFACE)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let active_paths: Vec<zbus::zvariant::OwnedObjectPath> = nm
            .get::<Vec<zbus::zvariant::OwnedObjectPath>>("ActiveConnections")
            .await
            .ok_or_else(|| anyhow::anyhow!("failed to get active connections"))?;

        for path in &active_paths {
            let path_str = path.to_string();
            let Ok(active) = DbusPropertyGroup::new(
                conn,
                NM_SERVICE,
                &path_str,
                "org.freedesktop.NetworkManager.Connection.Active",
            )
            .await
            else {
                continue;
            };

            let active_uuid = active.get::<String>("Uuid").await.unwrap_or_default();
            if active_uuid == uuid {
                let active_obj = zbus::zvariant::ObjectPath::try_from(path_str.as_str())
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                nm.call_void("DeactivateConnection", &(&active_obj,))
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                return Ok(json!(null));
            }
        }

        Err(anyhow::anyhow!(
            "no active connection found with uuid: {uuid}"
        ))
    }

    async fn forget(
        &self,
        conn: &zbus::Connection,
        uuid: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let settings = DbusPropertyGroup::new(conn, NM_SERVICE, NM_SETTINGS_PATH, NM_SETTINGS_IFACE)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let conn_path: zbus::zvariant::OwnedObjectPath = settings
            .call("GetConnectionByUuid", &(uuid,))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        let sc = DbusPropertyGroup::new(
            conn,
            NM_SERVICE,
            &conn_path.to_string(),
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

        sc.call_void("Delete", &())
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        Ok(json!(null))
    }
}

pub struct NetworkProviderFactory;

impl ProviderFactory for NetworkProviderFactory {
    fn name(&self) -> &'static str {
        NAME
    }
    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }
    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }
    fn create(&self) -> Box<dyn Provider> {
        Box::new(NetworkProvider {
            status: NetworkStatus::default(),
            access_points: Vec::new(),
            connections: Vec::new(),
            devices: Vec::new(),
            saved_vpns: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- NM enum conversions --

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
        assert_eq!(device_type_str(14), "other");
    }

    #[test]
    fn connection_state_values() {
        assert_eq!(connection_state_str(1), "activating");
        assert_eq!(connection_state_str(2), "activated");
        assert_eq!(connection_state_str(3), "deactivating");
        assert_eq!(connection_state_str(0), "unknown");
    }

    #[test]
    fn connection_type_mapping() {
        assert_eq!(connection_type_str("802-11-wireless"), "wifi");
        assert_eq!(connection_type_str("802-3-ethernet"), "ethernet");
        assert_eq!(connection_type_str("vpn"), "vpn");
        assert_eq!(connection_type_str("wireguard"), "wireguard");
        assert_eq!(connection_type_str("bridge"), "other");
        assert_eq!(connection_type_str(""), "other");
    }

    // -- WiFi security detection --

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
    fn security_wpa3_over_wpa2() {
        // WPA3 takes precedence when both flags set
        assert_eq!(ap_security(0, 0, 0x500), "wpa3");
    }

    // -- WiFi icon thresholds --

    #[test]
    fn wifi_icon_excellent() {
        assert_eq!(wifi_icon(100), "network-wireless-signal-excellent-symbolic");
        assert_eq!(wifi_icon(75), "network-wireless-signal-excellent-symbolic");
    }

    #[test]
    fn wifi_icon_good() {
        assert_eq!(wifi_icon(74), "network-wireless-signal-good-symbolic");
        assert_eq!(wifi_icon(50), "network-wireless-signal-good-symbolic");
    }

    #[test]
    fn wifi_icon_ok() {
        assert_eq!(wifi_icon(49), "network-wireless-signal-ok-symbolic");
        assert_eq!(wifi_icon(25), "network-wireless-signal-ok-symbolic");
    }

    #[test]
    fn wifi_icon_weak() {
        assert_eq!(wifi_icon(24), "network-wireless-signal-weak-symbolic");
        assert_eq!(wifi_icon(1), "network-wireless-signal-weak-symbolic");
    }

    #[test]
    fn wifi_icon_none() {
        assert_eq!(wifi_icon(0), "network-wireless-signal-none-symbolic");
    }

    // -- Icon resolution from provider state --

    fn make_provider() -> NetworkProvider {
        NetworkProvider {
            status: NetworkStatus::default(),
            access_points: Vec::new(),
            connections: Vec::new(),
            devices: Vec::new(),
            saved_vpns: Vec::new(),
        }
    }

    #[test]
    fn icon_offline_when_disabled() {
        let mut p = make_provider();
        p.status.enabled = false;
        p.resolve_icon();
        assert_eq!(p.status.icon, "network-offline-symbolic");
    }

    #[test]
    fn icon_wifi_from_connected_ap() {
        let mut p = make_provider();
        p.status.enabled = true;
        p.connections.push(NetworkConnection {
            id: "Home".into(), uuid: "u1".into(),
            connection_type: "wifi".into(), device: "wlan0".into(),
            state: "activated".into(), vpn: false,
            ip4_address: None, gateway: None, dns: vec![], speed: 72,
        });
        p.access_points.push(WifiAccessPoint {
            ssid: "Home".into(), strength: 82, frequency: 5200,
            security: "wpa2".into(), connected: true, saved: true, uuid: Some("u1".into()),
        });
        p.resolve_icon();
        assert_eq!(p.status.icon, "network-wireless-signal-excellent-symbolic");
    }

    #[test]
    fn icon_wired_when_ethernet() {
        let mut p = make_provider();
        p.status.enabled = true;
        p.connections.push(NetworkConnection {
            id: "Wired".into(), uuid: "u1".into(),
            connection_type: "ethernet".into(), device: "enp3s0".into(),
            state: "activated".into(), vpn: false,
            ip4_address: None, gateway: None, dns: vec![], speed: 1000,
        });
        p.resolve_icon();
        assert_eq!(p.status.icon, "network-wired-symbolic");
    }

    #[test]
    fn icon_wifi_disabled() {
        let mut p = make_provider();
        p.status.enabled = true;
        p.status.wifi_enabled = false;
        p.resolve_icon();
        assert_eq!(p.status.icon, "network-wireless-disabled-symbolic");
    }

    #[test]
    fn icon_offline_when_enabled_but_no_connections() {
        let mut p = make_provider();
        p.status.enabled = true;
        p.status.wifi_enabled = true;
        p.resolve_icon();
        assert_eq!(p.status.icon, "network-offline-symbolic");
    }

    #[test]
    fn icon_wifi_prefers_over_ethernet() {
        let mut p = make_provider();
        p.status.enabled = true;
        p.connections.push(NetworkConnection {
            id: "Wired".into(), uuid: "u1".into(),
            connection_type: "ethernet".into(), device: "enp3s0".into(),
            state: "activated".into(), vpn: false,
            ip4_address: None, gateway: None, dns: vec![], speed: 1000,
        });
        p.connections.push(NetworkConnection {
            id: "Home".into(), uuid: "u2".into(),
            connection_type: "wifi".into(), device: "wlan0".into(),
            state: "activated".into(), vpn: false,
            ip4_address: None, gateway: None, dns: vec![], speed: 72,
        });
        p.access_points.push(WifiAccessPoint {
            ssid: "Home".into(), strength: 60, frequency: 5200,
            security: "wpa2".into(), connected: true, saved: true, uuid: Some("u2".into()),
        });
        p.resolve_icon();
        assert_eq!(p.status.icon, "network-wireless-signal-good-symbolic");
    }

    // -- Serialization contract (daemon → panel) --

    #[test]
    fn status_json_shape() {
        let status = NetworkStatus {
            connectivity: "full".into(),
            enabled: true,
            wifi_enabled: true,
            wifi_hw_enabled: true,
            primary_connection: "MyWiFi".into(),
            primary_type: "wifi".into(),
            metered: false,
            speed: 72,
            icon: "network-wireless-signal-excellent-symbolic".into(),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["connectivity"], "full");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["wifi_enabled"], true);
        assert_eq!(json["primary_connection"], "MyWiFi");
        assert_eq!(json["primary_type"], "wifi");
        assert_eq!(json["metered"], false);
        assert_eq!(json["speed"], 72);
        assert_eq!(json["icon"], "network-wireless-signal-excellent-symbolic");
    }

    #[test]
    fn access_point_json_shape() {
        let ap = WifiAccessPoint {
            ssid: "CoffeeShop".into(), strength: 65, frequency: 2437,
            security: "wpa2".into(), connected: false, saved: true,
            uuid: Some("abc-123".into()),
        };
        let json = serde_json::to_value(&ap).unwrap();
        assert_eq!(json["ssid"], "CoffeeShop");
        assert_eq!(json["strength"], 65);
        assert_eq!(json["frequency"], 2437);
        assert_eq!(json["security"], "wpa2");
        assert_eq!(json["connected"], false);
        assert_eq!(json["saved"], true);
        assert_eq!(json["uuid"], "abc-123");
    }

    #[test]
    fn connection_json_shape() {
        let conn = NetworkConnection {
            id: "Work VPN".into(), uuid: "vpn-uuid".into(),
            connection_type: "vpn".into(), device: "".into(),
            state: "activated".into(), vpn: true,
            ip4_address: Some("10.0.0.5".into()),
            gateway: Some("10.0.0.1".into()),
            dns: vec!["1.1.1.1".into()],
            speed: 0,
        };
        let json = serde_json::to_value(&conn).unwrap();
        assert_eq!(json["vpn"], true);
        assert_eq!(json["connection_type"], "vpn");
        assert_eq!(json["ip4_address"], "10.0.0.5");
        assert_eq!(json["dns"][0], "1.1.1.1");
    }

    #[test]
    fn saved_vpn_json_shape() {
        let vpn = SavedVpn {
            id: "Work".into(), uuid: "vpn-uuid".into(),
            connection_type: "vpn".into(), active: true,
            state: Some("activated".into()),
        };
        let json = serde_json::to_value(&vpn).unwrap();
        assert_eq!(json["id"], "Work");
        assert_eq!(json["active"], true);
        assert_eq!(json["state"], "activated");
    }

    #[test]
    fn device_json_shape() {
        let dev = NetworkDevice {
            interface: "enp3s0".into(), device_type: "ethernet".into(),
            state: "connected".into(), speed: 1000,
            carrier: Some(true),
        };
        let json = serde_json::to_value(&dev).unwrap();
        assert_eq!(json["interface"], "enp3s0");
        assert_eq!(json["carrier"], true);
        assert_eq!(json["speed"], 1000);
    }

}
