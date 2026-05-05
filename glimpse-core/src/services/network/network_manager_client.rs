#![allow(dead_code)]

use std::{collections::HashMap, time::Duration};

use anyhow::{Context, anyhow};
use futures_util::{StreamExt, future};
use tokio::{sync::mpsc, time::Instant};
use tokio_util::sync::CancellationToken;
use zbus::{
    MatchRule, MessageStream,
    message::Type,
    zvariant::{ObjectPath, OwnedValue, Value},
};

use crate::dbus::network_manager::{
    AccessPointProxy, ActiveConnectionProxy, DeviceProxy, DeviceWiredProxy, DeviceWirelessProxy,
    NetworkManagerProxy, SettingsConnectionProxy, SettingsProxy,
};

use super::{
    NetworkChangeReason, NetworkConnection, NetworkDevice, NetworkEvent,
    NetworkFailureClassification, NetworkSnapshot, NetworkStatus, SavedVpn, WifiAccessPoint,
    model::merge_change_reason,
};

const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const LISTENER_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Clone)]
pub struct NetworkManagerClient {
    conn: zbus::Connection,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SavedWifiProfile {
    id: String,
    uuid: String,
    ssid: String,
    has_inline_secret: bool,
}

impl NetworkManagerClient {
    pub fn new(conn: zbus::Connection) -> Self {
        Self { conn }
    }

    pub async fn scan(&self) -> anyhow::Result<NetworkSnapshot> {
        let manager = self.manager_proxy().await?;
        let mut status = NetworkStatus {
            connectivity: connectivity_text(manager.connectivity().await.unwrap_or(0)).into(),
            enabled: manager.networking_enabled().await.unwrap_or(false),
            wifi_enabled: manager.wireless_enabled().await.unwrap_or(false),
            wifi_hw_enabled: manager.wireless_hardware_enabled().await.unwrap_or(false),
            metered: matches!(manager.metered().await.unwrap_or(0), 1 | 3),
            icon: "network-offline-symbolic".into(),
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
                connection_type_text(&active.kind().await.unwrap_or_default()).into();
        }

        let devices = self.read_devices(&mut status).await?;
        let connections = self.read_connections(&mut status).await?;
        let wifi_access_points = self.read_access_points(&status, &connections).await?;
        let saved_vpns = self.read_saved_vpns(&connections).await?;
        resolve_icon(&mut status, &connections, &wifi_access_points);

        let snapshot =
            NetworkSnapshot::new(status, wifi_access_points, connections, devices, saved_vpns);
        tracing::debug!(
            devices = snapshot.devices.len(),
            connections = snapshot.connections.len(),
            wifi_access_points = snapshot.wifi_access_points.len(),
            saved_vpns = snapshot.saved_vpns.len(),
            "network: scan complete"
        );
        Ok(snapshot)
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<NetworkEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        tracing::info!("network: listener started");

        let mut properties = self.match_stream("PropertiesChanged").await?;
        let mut device_added = self.match_stream("DeviceAdded").await?;
        let mut device_removed = self.match_stream("DeviceRemoved").await?;
        let mut ap_added = self.match_stream("AccessPointAdded").await?;
        let mut ap_removed = self.match_stream("AccessPointRemoved").await?;

        let mut pending_reason: Option<NetworkChangeReason> = None;
        let mut debounce_deadline: Option<Instant> = None;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("network: listener stopping");
                    break;
                }
                message = properties.next() => {
                    match message {
                        Some(Ok(message)) if is_network_manager_message(&message) => {
                            tracing::debug!("network: properties changed signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, NetworkChangeReason::PropertiesChanged));
                            debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => tracing::warn!(error = %error, "network: properties stream error"),
                        None => break,
                    }
                }
                message = device_added.next() => {
                    match message {
                        Some(Ok(message)) if is_network_manager_message(&message) => {
                            tracing::debug!("network: device added signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, NetworkChangeReason::DeviceAdded));
                            debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => tracing::warn!(error = %error, "network: device-added stream error"),
                        None => break,
                    }
                }
                message = device_removed.next() => {
                    match message {
                        Some(Ok(message)) if is_network_manager_message(&message) => {
                            tracing::debug!("network: device removed signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, NetworkChangeReason::DeviceRemoved));
                            debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => tracing::warn!(error = %error, "network: device-removed stream error"),
                        None => break,
                    }
                }
                message = ap_added.next() => {
                    match message {
                        Some(Ok(message)) if is_network_manager_message(&message) => {
                            tracing::debug!("network: access point added signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, NetworkChangeReason::AccessPointAdded));
                            debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => tracing::warn!(error = %error, "network: access-point-added stream error"),
                        None => break,
                    }
                }
                message = ap_removed.next() => {
                    match message {
                        Some(Ok(message)) if is_network_manager_message(&message) => {
                            tracing::debug!("network: access point removed signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, NetworkChangeReason::AccessPointRemoved));
                            debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => tracing::warn!(error = %error, "network: access-point-removed stream error"),
                        None => break,
                    }
                }
                _ = async {
                    match debounce_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => future::pending::<()>().await,
                    }
                }, if debounce_deadline.is_some() => {
                    let reason = pending_reason.take().unwrap_or(NetworkChangeReason::Mixed);
                    debounce_deadline = None;
                    tracing::debug!(reason = %reason, "network: change event emitted");
                    if events.send(NetworkEvent::Changed { reason }).await.is_err() {
                        tracing::info!("network: listener receiver dropped");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn set_wifi_enabled(&self, enabled: bool) -> anyhow::Result<()> {
        tracing::info!(enabled, "network: set wifi enabled requested");
        self.manager_proxy()
            .await?
            .set_wireless_enabled(enabled)
            .await
            .context("failed to set wifi enabled")?;
        tracing::info!(enabled, "network: set wifi enabled succeeded");
        Ok(())
    }

    pub async fn request_scan(&self) -> anyhow::Result<()> {
        tracing::debug!("network: scan requested");
        let device_paths = self.manager_proxy().await?.get_devices().await?;
        for device_path in device_paths {
            let device = self.device_proxy(device_path.as_str()).await?;
            if device.device_type().await.unwrap_or(0) != 2 {
                continue;
            }
            let wireless = self.wireless_device_proxy(device_path.as_str()).await?;
            if let Err(error) = wireless.request_scan(HashMap::new()).await {
                tracing::debug!(error = %error, path = %device_path, "network: wifi scan request skipped");
            }
        }
        Ok(())
    }

    pub async fn connect_access_point(
        &self,
        ssid: &str,
        access_point_path: &str,
    ) -> anyhow::Result<()> {
        tracing::info!(ssid, path = %access_point_path, "network: connect wifi requested");
        let device_path = self
            .wifi_device_for_access_point(access_point_path)
            .await?
            .ok_or_else(|| anyhow!("no wifi device found for access point"))?;

        let mut settings: HashMap<String, HashMap<String, OwnedValue>> = HashMap::new();
        settings.insert(
            "connection".into(),
            HashMap::from([("type".into(), owned_value("802-11-wireless"))]),
        );
        settings.insert(
            "802-11-wireless".into(),
            HashMap::from([("ssid".into(), owned_value(ssid.as_bytes().to_vec()))]),
        );

        let manager = self.manager_proxy().await?;
        let device = ObjectPath::try_from(device_path.as_str())?;
        let access_point = ObjectPath::try_from(access_point_path)?;
        manager
            .add_and_activate_connection2(settings, device, access_point, HashMap::new())
            .await
            .context("failed to connect wifi access point")?;
        tracing::info!(ssid, path = %access_point_path, "network: connect wifi requested successfully");
        Ok(())
    }

    pub async fn connect_uuid(&self, uuid: &str) -> anyhow::Result<()> {
        tracing::info!(uuid, "network: connect saved connection requested");
        let settings = self.settings_proxy().await?;
        let connection_path = settings.get_connection_by_uuid(uuid).await?;
        let connection_settings = self
            .settings_connection_proxy(connection_path.as_str())
            .await?
            .get_settings()
            .await
            .unwrap_or_default();

        let manager = self.manager_proxy().await?;
        let connection = ObjectPath::try_from(connection_path.as_str())?;
        let (device, specific_object) = match saved_wifi_ssid(&connection_settings) {
            Some(ssid) => self.resolve_wifi_activation_target(&ssid).await?,
            None => {
                let empty = ObjectPath::try_from("/")?;
                (empty.clone(), empty)
            }
        };
        manager
            .activate_connection(connection, device, specific_object)
            .await
            .context("failed to activate saved connection")?;
        tracing::info!(uuid, "network: connect saved connection succeeded");
        Ok(())
    }

    pub async fn disconnect(&self, uuid: &str) -> anyhow::Result<()> {
        tracing::info!(uuid, "network: disconnect requested");
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
                    .await
                    .context("failed to deactivate connection")?;
                tracing::info!(uuid, "network: disconnect succeeded");
                return Ok(());
            }
        }

        tracing::debug!(
            uuid,
            "network: disconnect skipped; connection is not active"
        );
        Ok(())
    }

    pub async fn forget(&self, uuid: &str) -> anyhow::Result<()> {
        tracing::info!(uuid, "network: forget requested");
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
                    .await
                    .with_context(|| format!("failed to delete wifi profile {profile_path}"))?;
            }
            tracing::info!(ssid, "network: forgot wifi profiles");
            return Ok(());
        }

        self.settings_connection_proxy(connection_path.as_str())
            .await?
            .delete()
            .await
            .context("failed to delete connection")?;
        tracing::info!(uuid, "network: forget succeeded");
        Ok(())
    }

    async fn read_devices(&self, status: &mut NetworkStatus) -> anyhow::Result<Vec<NetworkDevice>> {
        let mut devices = Vec::new();
        let device_paths = self.manager_proxy().await?.get_devices().await?;
        for path in device_paths {
            let device = self.device_proxy(path.as_str()).await?;
            let device_type = device_type_text(device.device_type().await.unwrap_or(0));
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
                state: device_state_text(state).into(),
                failure: device_failure_classification(device_type, state, state_reason),
                speed,
                carrier,
                hardware_address: device
                    .hw_address()
                    .await
                    .ok()
                    .filter(|value| !value.is_empty()),
                driver: device.driver().await.ok().filter(|value| !value.is_empty()),
                managed: device.managed().await.unwrap_or(true),
                mtu: device.mtu().await.ok().filter(|value| *value > 0),
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

            let mut device_path = String::new();
            let mut device_name = String::new();
            let mut speed = 0;
            if let Some(path) = active.devices().await.unwrap_or_default().first() {
                device_path = path.to_string();
                let device = self.device_proxy(path.as_str()).await?;
                device_name = device.interface().await.unwrap_or_default();
                speed = match device_type_text(device.device_type().await.unwrap_or(0)) {
                    "ethernet" => self
                        .wired_device_proxy(path.as_str())
                        .await?
                        .speed()
                        .await
                        .unwrap_or(0),
                    "wifi" => {
                        self.wireless_device_proxy(path.as_str())
                            .await?
                            .bitrate()
                            .await
                            .unwrap_or(0)
                            / 1000
                    }
                    _ => 0,
                };
            }

            let connection_type = connection_type_text(&raw_type);
            if (connection_type == "wifi" || connection_type == "ethernet") && state == 2 {
                status.speed = speed;
            }

            connections.push(NetworkConnection {
                active_path: path.to_string(),
                settings_path: self.settings_path_for_uuid(&uuid).await.ok().flatten(),
                id,
                uuid,
                connection_type: connection_type.into(),
                device_path,
                device: device_name,
                state: connection_state_text(state).into(),
                failure: active_connection_failure_classification(state, state_reason),
                vpn,
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

        let connected_access_points = self.connected_access_point_uuids(connections).await?;
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

                let flags = access_point.flags().await.unwrap_or(0);
                let wpa_flags = access_point.wpa_flags().await.unwrap_or(0);
                let rsn_flags = access_point.rsn_flags().await.unwrap_or(0);
                let connected_uuid = connected_access_point_uuid(
                    &connected_access_points,
                    access_point_path.as_str(),
                );
                let connected = connected_uuid.is_some();
                let saved_profiles = saved_wifi.get(&ssid);
                let saved_uuid = preferred_saved_wifi_profile(
                    saved_profiles.map(|profiles| profiles.as_slice()),
                    &ssid,
                )
                .map(|profile| profile.uuid.clone());

                access_points.push(WifiAccessPoint {
                    path: access_point_path.to_string(),
                    device_path: device_path.to_string(),
                    ssid: ssid.clone(),
                    strength: access_point.strength().await.unwrap_or(0),
                    frequency: access_point.frequency().await.unwrap_or(0),
                    security: ap_security(flags, wpa_flags, rsn_flags).into(),
                    connected,
                    saved: saved_profiles.is_some() || connected,
                    uuid: connected_uuid.or(saved_uuid),
                });
            }
        }
        Ok(access_points)
    }

    async fn connected_access_point_uuids(
        &self,
        connections: &[NetworkConnection],
    ) -> anyhow::Result<HashMap<String, String>> {
        let mut access_points = HashMap::new();
        for connection in connections {
            if connection.connection_type != "wifi"
                || connection.state != "activated"
                || connection.device_path.is_empty()
            {
                continue;
            }

            let wireless = match self.wireless_device_proxy(&connection.device_path).await {
                Ok(wireless) => wireless,
                Err(error) => {
                    tracing::debug!(
                        error = %error,
                        device = %connection.device_path,
                        "network: active wifi device lookup failed"
                    );
                    continue;
                }
            };
            let active_access_point = wireless
                .active_access_point()
                .await
                .map(|path| path.to_string())
                .unwrap_or_default();
            if is_real_path(&active_access_point) {
                access_points.insert(active_access_point, connection.uuid.clone());
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

            let Some(ssid) = saved_wifi_ssid(&settings) else {
                continue;
            };
            let uuid = connection_section
                .get("uuid")
                .and_then(owned_value_to_string)
                .unwrap_or_default();
            if ssid.is_empty() || uuid.is_empty() {
                continue;
            }

            saved
                .entry(ssid.clone())
                .or_default()
                .push(SavedWifiProfile {
                    id: connection_section
                        .get("id")
                        .and_then(owned_value_to_string)
                        .unwrap_or_default(),
                    uuid,
                    ssid,
                    has_inline_secret: settings
                        .get("802-11-wireless-security")
                        .and_then(|security| security.get("psk"))
                        .and_then(owned_value_to_string)
                        .is_some_and(|psk| !psk.is_empty()),
                });
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

            let uuid = connection_section
                .get("uuid")
                .and_then(owned_value_to_string)
                .unwrap_or_default();
            let active_state = active_vpns.get(&uuid);
            saved_vpns.push(SavedVpn {
                id: connection_section
                    .get("id")
                    .and_then(owned_value_to_string)
                    .unwrap_or_default(),
                uuid,
                settings_path: path.to_string(),
                connection_type: connection_type_text(&connection_type).into(),
                active: active_state.is_some(),
                state: active_state.cloned(),
            });
        }
        Ok(saved_vpns)
    }

    async fn wifi_device_for_access_point(
        &self,
        access_point_path: &str,
    ) -> anyhow::Result<Option<String>> {
        let device_paths = self.manager_proxy().await?.get_devices().await?;
        for device_path in device_paths {
            let device = self.device_proxy(device_path.as_str()).await?;
            if device.device_type().await.unwrap_or(0) != 2 {
                continue;
            }
            let wireless = self.wireless_device_proxy(device_path.as_str()).await?;
            if wireless
                .get_all_access_points()
                .await
                .unwrap_or_default()
                .iter()
                .any(|candidate| candidate.as_str() == access_point_path)
            {
                return Ok(Some(device_path.to_string()));
            }
        }
        Ok(None)
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

    async fn manager_proxy(&self) -> anyhow::Result<NetworkManagerProxy<'_>> {
        NetworkManagerProxy::new(&self.conn)
            .await
            .map_err(Into::into)
    }

    async fn settings_proxy(&self) -> anyhow::Result<SettingsProxy<'_>> {
        SettingsProxy::new(&self.conn).await.map_err(Into::into)
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

fn connectivity_text(value: u32) -> &'static str {
    match value {
        1 => "none",
        2 => "portal",
        3 => "limited",
        4 => "full",
        _ => "unknown",
    }
}

fn device_state_text(value: u32) -> &'static str {
    match value {
        100 => "connected",
        20 => "unavailable",
        110 => "deactivating",
        30..=90 => "connecting",
        _ => "disconnected",
    }
}

fn device_type_text(value: u32) -> &'static str {
    match value {
        1 => "ethernet",
        2 => "wifi",
        29 => "wireguard",
        _ => "other",
    }
}

fn connection_state_text(value: u32) -> &'static str {
    match value {
        1 => "activating",
        2 => "activated",
        3 => "deactivating",
        _ => "unknown",
    }
}

fn connection_type_text(value: &str) -> &'static str {
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

fn connected_access_point_uuid(
    connected_access_points: &HashMap<String, String>,
    access_point_path: &str,
) -> Option<String> {
    connected_access_points.get(access_point_path).cloned()
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

fn is_network_manager_message(message: &zbus::message::Message) -> bool {
    let header = message.header();
    let Some(path) = header.path() else {
        return false;
    };
    if !path.as_str().starts_with("/org/freedesktop/NetworkManager") {
        return false;
    }

    let Some(member) = header.member() else {
        return true;
    };
    if member.as_str() != "PropertiesChanged" {
        return true;
    }

    match message
        .body()
        .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
    {
        Ok((interface, changed, invalidated)) => network_properties_are_relevant(
            &interface,
            changed.keys().map(String::as_str),
            invalidated.iter().map(String::as_str),
        ),
        Err(error) => {
            tracing::debug!(%error, "network: failed to inspect properties changed body");
            true
        }
    }
}

fn network_properties_are_relevant<'a>(
    interface: &str,
    changed: impl Iterator<Item = &'a str>,
    invalidated: impl Iterator<Item = &'a str>,
) -> bool {
    changed
        .chain(invalidated)
        .any(|property| network_property_is_relevant(interface, property))
}

fn network_property_is_relevant(interface: &str, property: &str) -> bool {
    match interface {
        "org.freedesktop.NetworkManager" => matches!(
            property,
            "Connectivity"
                | "NetworkingEnabled"
                | "WirelessEnabled"
                | "WirelessHardwareEnabled"
                | "Metered"
                | "PrimaryConnection"
                | "ActiveConnections"
        ),
        "org.freedesktop.NetworkManager.Device" => {
            matches!(property, "State" | "StateReason" | "Interface" | "Managed")
        }
        "org.freedesktop.NetworkManager.Device.Wired" => {
            matches!(property, "Speed" | "Carrier")
        }
        "org.freedesktop.NetworkManager.Device.Wireless" => {
            matches!(property, "Bitrate" | "ActiveAccessPoint" | "AccessPoints")
        }
        "org.freedesktop.NetworkManager.AccessPoint" => matches!(
            property,
            "Ssid" | "Strength" | "Frequency" | "Flags" | "WpaFlags" | "RsnFlags"
        ),
        "org.freedesktop.NetworkManager.Connection.Active" => matches!(
            property,
            "Id" | "Uuid" | "Type" | "State" | "StateReason" | "Vpn" | "Devices"
        ),
        _ => false,
    }
}

fn is_real_path(path: &str) -> bool {
    !path.is_empty() && path != "/"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_mappings_are_stable() {
        assert_eq!(connectivity_text(4), "full");
        assert_eq!(device_type_text(2), "wifi");
        assert_eq!(connection_type_text("802-11-wireless"), "wifi");
        assert_eq!(connection_state_text(2), "activated");
    }

    #[test]
    fn wifi_icon_tracks_strength() {
        assert_eq!(wifi_icon(0), "network-wireless-signal-none-symbolic");
        assert_eq!(wifi_icon(30), "network-wireless-signal-ok-symbolic");
        assert_eq!(wifi_icon(80), "network-wireless-signal-excellent-symbolic");
    }

    #[test]
    fn active_connection_failures_are_classified() {
        assert_eq!(
            active_connection_failure_classification(1, 10),
            Some(NetworkFailureClassification::AuthenticationFailed)
        );
        assert_eq!(active_connection_failure_classification(2, 10), None);
    }

    #[test]
    fn connected_access_point_detection_uses_access_point_path_not_ssid() {
        let connected = HashMap::from([(
            "/org/freedesktop/NetworkManager/AccessPoint/7".into(),
            "connection-uuid".into(),
        )]);

        assert_eq!(
            connected_access_point_uuid(
                &connected,
                "/org/freedesktop/NetworkManager/AccessPoint/7"
            ),
            Some("connection-uuid".into())
        );
        assert_eq!(
            connected_access_point_uuid(
                &connected,
                "/org/freedesktop/NetworkManager/AccessPoint/8"
            ),
            None
        );
    }

    #[test]
    fn network_property_filter_ignores_irrelevant_property_changes() {
        assert!(network_property_is_relevant(
            "org.freedesktop.NetworkManager.AccessPoint",
            "Strength"
        ));
        assert!(network_property_is_relevant(
            "org.freedesktop.NetworkManager",
            "PrimaryConnection"
        ));
        assert!(!network_property_is_relevant(
            "org.freedesktop.NetworkManager.Device.Statistics",
            "RxBytes"
        ));
        assert!(!network_property_is_relevant(
            "org.freedesktop.DBus.Introspectable",
            "Anything"
        ));
    }
}
