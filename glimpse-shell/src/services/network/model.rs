use std::fmt;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkConnection {
    pub active_path: String,
    pub settings_path: Option<String>,
    pub id: String,
    pub uuid: String,
    pub connection_type: String,
    pub device_path: String,
    pub device: String,
    pub state: String,
    pub failure: Option<NetworkFailureClassification>,
    pub vpn: bool,
    pub speed: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedVpn {
    pub id: String,
    pub uuid: String,
    pub settings_path: String,
    pub connection_type: String,
    pub active: bool,
    pub state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkFailureClassification {
    AuthenticationFailed,
    MissingSecrets,
    Timeout,
    NetworkNotFound,
    ConfigurationFailed,
    ConnectionRemoved,
    Disconnected,
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
    pub fn new(
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
pub enum NetworkEvent {
    Changed { reason: NetworkChangeReason },
}

pub fn merge_change_reason(
    current: Option<NetworkChangeReason>,
    next: NetworkChangeReason,
) -> NetworkChangeReason {
    match current {
        None => next,
        Some(current) if current == next => current,
        Some(_) => NetworkChangeReason::Mixed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wifi(ssid: &str, strength: u8, connected: bool, saved: bool) -> WifiAccessPoint {
        WifiAccessPoint {
            ssid: ssid.into(),
            strength,
            connected,
            saved,
            ..WifiAccessPoint::default()
        }
    }

    #[test]
    fn snapshot_sorts_wifi_connections_devices_and_vpns() {
        let snapshot = NetworkSnapshot::new(
            NetworkStatus::default(),
            vec![
                wifi("Cafe", 80, false, false),
                wifi("Home", 30, true, true),
                wifi("Office", 95, false, true),
            ],
            vec![
                NetworkConnection {
                    id: "vpn".into(),
                    ..NetworkConnection::default()
                },
                NetworkConnection {
                    id: "ethernet".into(),
                    ..NetworkConnection::default()
                },
            ],
            vec![
                NetworkDevice {
                    interface: "wlan0".into(),
                    ..NetworkDevice::default()
                },
                NetworkDevice {
                    interface: "eth0".into(),
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

        assert_eq!(
            snapshot
                .wifi_access_points
                .iter()
                .map(|ap| ap.ssid.as_str())
                .collect::<Vec<_>>(),
            ["Home", "Office", "Cafe"]
        );
        assert_eq!(snapshot.connections[0].id, "ethernet");
        assert_eq!(snapshot.devices[0].interface, "eth0");
        assert_eq!(snapshot.saved_vpns[0].id, "a");
    }

    #[test]
    fn change_reason_display_is_stable() {
        assert_eq!(
            NetworkChangeReason::PropertiesChanged.to_string(),
            "properties-changed"
        );
        assert_eq!(
            NetworkChangeReason::AccessPointAdded.to_string(),
            "access-point-added"
        );
        assert_eq!(NetworkChangeReason::Mixed.to_string(), "mixed");
    }

    #[test]
    fn merge_change_reason_keeps_same_reason_and_marks_mixed_bursts() {
        assert_eq!(
            merge_change_reason(None, NetworkChangeReason::DeviceAdded),
            NetworkChangeReason::DeviceAdded
        );
        assert_eq!(
            merge_change_reason(
                Some(NetworkChangeReason::DeviceAdded),
                NetworkChangeReason::DeviceAdded
            ),
            NetworkChangeReason::DeviceAdded
        );
        assert_eq!(
            merge_change_reason(
                Some(NetworkChangeReason::DeviceAdded),
                NetworkChangeReason::PropertiesChanged
            ),
            NetworkChangeReason::Mixed
        );
    }
}
