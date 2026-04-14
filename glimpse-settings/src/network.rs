use glimpse::network::{
    protocol::NetworkServiceState,
    provider::{NetworkConnection, NetworkDevice, NetworkSnapshot, SavedVpn, WifiAccessPoint},
};

#[derive(Debug, Clone)]
pub struct NetworkPageState {
    service_state: NetworkServiceState,
    selected_wifi_adapter_path: Option<String>,
}

impl NetworkPageState {
    pub fn from_service_state(service_state: NetworkServiceState) -> Self {
        let selected_wifi_adapter_path = first_wifi_adapter_path(&service_state.snapshot);
        Self {
            service_state,
            selected_wifi_adapter_path,
        }
    }

    pub fn apply_service_state(&mut self, service_state: NetworkServiceState) {
        let next_selection = self
            .selected_wifi_adapter_path
            .as_deref()
            .filter(|path| wifi_adapter_exists(&service_state.snapshot, path))
            .map(str::to_owned)
            .or_else(|| first_wifi_adapter_path(&service_state.snapshot));

        self.service_state = service_state;
        self.selected_wifi_adapter_path = next_selection;
    }

    pub fn service_state(&self) -> &NetworkServiceState {
        &self.service_state
    }

    pub fn select_wifi_adapter(&mut self, path: &str) {
        if wifi_adapter_exists(&self.service_state.snapshot, path) {
            self.selected_wifi_adapter_path = Some(path.to_owned());
        }
    }

    pub fn selected_wifi_adapter_path(&self) -> Option<&str> {
        self.selected_wifi_adapter_path.as_deref()
    }

    pub fn wifi_adapters(&self) -> Vec<&NetworkDevice> {
        self.service_state
            .snapshot
            .devices
            .iter()
            .filter(|device| device.device_type == "wifi")
            .collect()
    }

    pub fn ethernet_devices(&self) -> Vec<&NetworkDevice> {
        self.service_state
            .snapshot
            .devices
            .iter()
            .filter(|device| device.device_type == "ethernet")
            .collect()
    }

    pub fn saved_vpns(&self) -> &[SavedVpn] {
        &self.service_state.snapshot.saved_vpns
    }

    pub fn adapters(&self) -> &[NetworkDevice] {
        &self.service_state.snapshot.devices
    }

    pub fn show_wifi_adapter_selector(&self) -> bool {
        self.wifi_adapters().len() > 1
    }

    pub fn visible_wifi_access_points(&self) -> Vec<&WifiAccessPoint> {
        let Some(selected) = self.selected_wifi_adapter_path() else {
            return Vec::new();
        };

        self.service_state
            .snapshot
            .wifi_access_points
            .iter()
            .filter(|access_point| access_point.device_path == selected)
            .collect()
    }

    pub fn selected_wifi_adapter(&self) -> Option<&NetworkDevice> {
        let selected = self.selected_wifi_adapter_path()?;
        self.device(selected)
    }

    pub fn primary_connection(&self) -> Option<&NetworkConnection> {
        let status = &self.service_state.snapshot.status;
        self.service_state
            .snapshot
            .connections
            .iter()
            .find(|connection| connection.id == status.primary_connection)
            .or_else(|| self.service_state.snapshot.connections.first())
    }

    pub fn access_point(&self, path: &str) -> Option<&WifiAccessPoint> {
        self.service_state
            .snapshot
            .wifi_access_points
            .iter()
            .find(|access_point| access_point.path == path)
    }

    pub fn device(&self, path: &str) -> Option<&NetworkDevice> {
        self.service_state
            .snapshot
            .devices
            .iter()
            .find(|device| device.path == path)
    }

    pub fn connection_by_uuid(&self, uuid: &str) -> Option<&NetworkConnection> {
        self.service_state
            .snapshot
            .connections
            .iter()
            .find(|connection| connection.uuid == uuid)
    }

    pub fn connection_for_device(&self, interface: &str) -> Option<&NetworkConnection> {
        self.service_state
            .snapshot
            .connections
            .iter()
            .find(|connection| connection.device == interface)
    }

    pub fn vpn(&self, uuid: &str) -> Option<&SavedVpn> {
        self.service_state
            .snapshot
            .saved_vpns
            .iter()
            .find(|vpn| vpn.uuid == uuid)
    }
}

fn first_wifi_adapter_path(snapshot: &NetworkSnapshot) -> Option<String> {
    snapshot
        .devices
        .iter()
        .find(|device| device.device_type == "wifi")
        .map(|device| device.path.clone())
}

fn wifi_adapter_exists(snapshot: &NetworkSnapshot, path: &str) -> bool {
    snapshot
        .devices
        .iter()
        .any(|device| device.device_type == "wifi" && device.path == path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::network::{
        protocol::{NetworkServiceHealth, NetworkServiceState},
        provider::{NetworkStatus, SavedVpn},
    };

    fn wifi_device(path: &str, interface: &str) -> NetworkDevice {
        NetworkDevice {
            path: path.into(),
            interface: interface.into(),
            device_type: "wifi".into(),
            state: "connected".into(),
            failure: None,
            speed: 300,
            carrier: None,
            hardware_address: None,
            driver: None,
            managed: true,
            mtu: Some(1500),
            hotspot_supported: true,
        }
    }

    fn ethernet_device(path: &str, interface: &str) -> NetworkDevice {
        NetworkDevice {
            path: path.into(),
            interface: interface.into(),
            device_type: "ethernet".into(),
            state: "connected".into(),
            failure: None,
            speed: 1000,
            carrier: Some(true),
            hardware_address: None,
            driver: None,
            managed: true,
            mtu: Some(1500),
            hotspot_supported: false,
        }
    }

    fn service_state(
        devices: Vec<NetworkDevice>,
        wifi_access_points: Vec<WifiAccessPoint>,
    ) -> NetworkServiceState {
        NetworkServiceState {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                status: NetworkStatus::default(),
                wifi_access_points,
                connections: Vec::new(),
                devices,
                saved_vpns: vec![SavedVpn {
                    id: "Work".into(),
                    uuid: "vpn-1".into(),
                    settings_path: "/settings/1".into(),
                    connection_type: "vpn".into(),
                    active: false,
                    state: None,
                }],
            },
            prompt: None,
            active_action: None,
            scanning: false,
        }
    }

    #[test]
    fn retains_selected_wifi_adapter_when_it_still_exists() {
        let mut page = NetworkPageState::from_service_state(service_state(
            vec![wifi_device("/wifi/1", "wlan0"), wifi_device("/wifi/2", "wlan1")],
            Vec::new(),
        ));
        page.select_wifi_adapter("/wifi/2");

        page.apply_service_state(service_state(
            vec![wifi_device("/wifi/1", "wlan0"), wifi_device("/wifi/2", "wlan1")],
            Vec::new(),
        ));

        assert_eq!(page.selected_wifi_adapter_path(), Some("/wifi/2"));
    }

    #[test]
    fn falls_back_to_first_wifi_adapter_when_selection_disappears() {
        let mut page = NetworkPageState::from_service_state(service_state(
            vec![wifi_device("/wifi/1", "wlan0"), wifi_device("/wifi/2", "wlan1")],
            Vec::new(),
        ));
        page.select_wifi_adapter("/wifi/2");

        page.apply_service_state(service_state(
            vec![wifi_device("/wifi/1", "wlan0")],
            Vec::new(),
        ));

        assert_eq!(page.selected_wifi_adapter_path(), Some("/wifi/1"));
    }

    #[test]
    fn filters_access_points_by_selected_wifi_adapter() {
        let page = NetworkPageState::from_service_state(service_state(
            vec![wifi_device("/wifi/1", "wlan0"), wifi_device("/wifi/2", "wlan1")],
            vec![
                WifiAccessPoint {
                    path: "/ap/1".into(),
                    device_path: "/wifi/1".into(),
                    ssid: "One".into(),
                    strength: 80,
                    frequency: 5000,
                    security: "wpa2".into(),
                    connected: false,
                    saved: false,
                    uuid: None,
                },
                WifiAccessPoint {
                    path: "/ap/2".into(),
                    device_path: "/wifi/2".into(),
                    ssid: "Two".into(),
                    strength: 70,
                    frequency: 2412,
                    security: "open".into(),
                    connected: false,
                    saved: false,
                    uuid: None,
                },
            ],
        ));

        assert_eq!(page.visible_wifi_access_points().len(), 1);
        assert_eq!(page.visible_wifi_access_points()[0].ssid, "One");
    }

    #[test]
    fn ethernet_devices_are_listed_separately() {
        let page = NetworkPageState::from_service_state(service_state(
            vec![wifi_device("/wifi/1", "wlan0"), ethernet_device("/eth/1", "enp1s0")],
            Vec::new(),
        ));

        assert_eq!(page.wifi_adapters().len(), 1);
        assert_eq!(page.ethernet_devices().len(), 1);
    }
}
