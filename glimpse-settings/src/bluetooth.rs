use glimpse::{
    bluetooth::protocol::BluetoothServiceState,
    providers::bluetooth::{BluetoothAdapter, BluetoothDevice},
};

#[derive(Debug, Clone)]
pub struct BluetoothPageState {
    service_state: BluetoothServiceState,
    selected_adapter_path: Option<String>,
}

impl BluetoothPageState {
    pub fn from_service_state(service_state: BluetoothServiceState) -> Self {
        let selected_adapter_path = first_adapter_path(&service_state);
        Self {
            service_state,
            selected_adapter_path,
        }
    }

    pub fn apply_service_state(&mut self, service_state: BluetoothServiceState) {
        let next_selection = self
            .selected_adapter_path
            .as_deref()
            .filter(|path| adapter_exists(&service_state, path))
            .map(str::to_owned)
            .or_else(|| first_adapter_path(&service_state));

        self.service_state = service_state;
        self.selected_adapter_path = next_selection;
    }

    pub fn select_adapter(&mut self, adapter_path: &str) {
        if adapter_exists(&self.service_state, adapter_path) {
            self.selected_adapter_path = Some(adapter_path.to_owned());
        }
    }

    pub fn selected_adapter_path(&self) -> Option<&str> {
        self.selected_adapter_path.as_deref()
    }

    pub fn selected_adapter(&self) -> Option<&BluetoothAdapter> {
        let selected = self.selected_adapter_path()?;
        self.service_state
            .snapshot
            .adapters
            .iter()
            .find(|adapter| adapter.path == selected)
    }

    pub fn adapters(&self) -> &[BluetoothAdapter] {
        &self.service_state.snapshot.adapters
    }

    pub fn show_adapter_selector(&self) -> bool {
        self.adapters().len() > 1
    }

    pub fn visible_devices(&self) -> Vec<&BluetoothDevice> {
        let Some(selected_adapter) = self.selected_adapter_path() else {
            return Vec::new();
        };

        let mut visible = self
            .service_state
            .snapshot
            .devices
            .iter()
            .filter(|device| device.adapter == selected_adapter && is_visible_device(device))
            .collect::<Vec<_>>();

        visible.sort_by(|left, right| {
            right
                .connected
                .cmp(&left.connected)
                .then(right.paired.cmp(&left.paired))
                .then(right.rssi.unwrap_or(i16::MIN).cmp(&left.rssi.unwrap_or(i16::MIN)))
        });

        visible
    }

    pub fn service_state(&self) -> &BluetoothServiceState {
        &self.service_state
    }

    pub fn device(&self, address: &str) -> Option<&BluetoothDevice> {
        self.service_state
            .snapshot
            .devices
            .iter()
            .find(|device| device.address == address)
    }

    pub fn adapter(&self, adapter_path: &str) -> Option<&BluetoothAdapter> {
        self.service_state
            .snapshot
            .adapters
            .iter()
            .find(|adapter| adapter.path == adapter_path)
    }

    pub fn set_global_powered(&mut self, powered: bool) {
        self.service_state.snapshot.status.powered = powered;
        for adapter in &mut self.service_state.snapshot.adapters {
            adapter.powered = powered;
        }
    }

    pub fn set_adapter_powered(&mut self, adapter_path: &str, powered: bool) {
        if let Some(adapter) = self
            .service_state
            .snapshot
            .adapters
            .iter_mut()
            .find(|adapter| adapter.path == adapter_path)
        {
            adapter.powered = powered;
        }
        self.service_state.snapshot.status.powered = self
            .service_state
            .snapshot
            .adapters
            .iter()
            .any(|adapter| adapter.powered);
    }

    pub fn set_adapter_discoverable(&mut self, adapter_path: &str, discoverable: bool) {
        if let Some(adapter) = self
            .service_state
            .snapshot
            .adapters
            .iter_mut()
            .find(|adapter| adapter.path == adapter_path)
        {
            adapter.discoverable = discoverable;
        }
    }
}

fn first_adapter_path(service_state: &BluetoothServiceState) -> Option<String> {
    service_state
        .snapshot
        .adapters
        .first()
        .map(|adapter| adapter.path.clone())
}

fn adapter_exists(service_state: &BluetoothServiceState, adapter_path: &str) -> bool {
    service_state
        .snapshot
        .adapters
        .iter()
        .any(|adapter| adapter.path == adapter_path)
}

fn is_visible_device(device: &BluetoothDevice) -> bool {
    if device.name.is_empty() || looks_like_mac(&device.name) {
        return device.connected || device.paired || device.trusted;
    }

    device.connected || device.paired || device.trusted || device.rssi.is_some()
}

fn looks_like_mac(value: &str) -> bool {
    let value = value.trim();
    if value.len() != 17 {
        return false;
    }

    let separator = if value.contains(':') {
        ':'
    } else if value.contains('-') {
        '-'
    } else {
        return false;
    };

    let parts = value.split(separator).collect::<Vec<_>>();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|ch| ch.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::{
        bluetooth::protocol::BluetoothServiceHealth,
        providers::bluetooth::{BluetoothDeviceType, BluetoothSnapshot, BluetoothStatus},
    };

    fn adapter(path: &str, name: &str, powered: bool, discoverable: bool) -> BluetoothAdapter {
        BluetoothAdapter {
            path: path.into(),
            name: name.into(),
            address: format!("ADDR-{name}"),
            powered,
            discovering: false,
            discoverable,
            pairable: true,
            address_type: "public".into(),
            class: 0,
            discoverable_timeout: 0,
            pairable_timeout: 0,
            modalias: String::new(),
            roles: Vec::new(),
            uuids: Vec::new(),
        }
    }

    fn device(
        address: &str,
        name: &str,
        adapter: &str,
        connected: bool,
        paired: bool,
        trusted: bool,
        rssi: Option<i16>,
    ) -> BluetoothDevice {
        BluetoothDevice {
            path: format!("{adapter}/{address}"),
            address: address.into(),
            alias: name.into(),
            name: name.into(),
            device_type: BluetoothDeviceType::Headphones,
            paired,
            connected,
            trusted,
            battery: None,
            rssi,
            class: 0,
            appearance: 0,
            adapter: adapter.into(),
        }
    }

    fn service_state(adapters: Vec<BluetoothAdapter>, devices: Vec<BluetoothDevice>) -> BluetoothServiceState {
        BluetoothServiceState {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus {
                    powered: adapters.iter().any(|adapter| adapter.powered),
                    discovering: false,
                    connected_count: devices.iter().filter(|device| device.connected).count() as u32,
                },
                adapters,
                devices,
            },
            prompt: None,
            active_action: None,
        }
    }

    #[test]
    fn picks_first_adapter_when_selection_is_missing() {
        let state = BluetoothPageState::from_service_state(service_state(
            vec![
                adapter("/org/bluez/hci0", "Built-in", true, true),
                adapter("/org/bluez/hci1", "USB", true, false),
            ],
            Vec::new(),
        ));

        assert_eq!(state.selected_adapter_path(), Some("/org/bluez/hci0"));
    }

    #[test]
    fn keeps_selected_adapter_when_it_still_exists() {
        let mut page = BluetoothPageState::from_service_state(service_state(
            vec![
                adapter("/org/bluez/hci0", "Built-in", true, true),
                adapter("/org/bluez/hci1", "USB", true, false),
            ],
            Vec::new(),
        ));
        page.select_adapter("/org/bluez/hci1");

        page.apply_service_state(service_state(
            vec![
                adapter("/org/bluez/hci0", "Built-in", true, true),
                adapter("/org/bluez/hci1", "USB", true, false),
            ],
            Vec::new(),
        ));

        assert_eq!(page.selected_adapter_path(), Some("/org/bluez/hci1"));
    }

    #[test]
    fn falls_back_when_selected_adapter_disappears() {
        let mut page = BluetoothPageState::from_service_state(service_state(
            vec![
                adapter("/org/bluez/hci0", "Built-in", true, true),
                adapter("/org/bluez/hci1", "USB", true, false),
            ],
            Vec::new(),
        ));
        page.select_adapter("/org/bluez/hci1");

        page.apply_service_state(service_state(
            vec![adapter("/org/bluez/hci0", "Built-in", true, true)],
            Vec::new(),
        ));

        assert_eq!(page.selected_adapter_path(), Some("/org/bluez/hci0"));
    }

    #[test]
    fn filters_devices_to_selected_adapter_and_sorts_connected_first() {
        let page = BluetoothPageState::from_service_state(service_state(
            vec![
                adapter("/org/bluez/hci0", "Built-in", true, true),
                adapter("/org/bluez/hci1", "USB", true, false),
            ],
            vec![
                device("AA", "Far Paired", "/org/bluez/hci0", false, true, false, Some(-80)),
                device("BB", "Nearby Unpaired", "/org/bluez/hci0", false, false, false, Some(-40)),
                device("CC", "Connected", "/org/bluez/hci0", true, true, true, Some(-90)),
                device("DD", "Other Adapter", "/org/bluez/hci1", true, true, true, Some(-10)),
            ],
        ));

        let visible = page.visible_devices();
        let names = visible.iter().map(|device| device.name.as_str()).collect::<Vec<_>>();

        assert_eq!(names, vec!["Connected", "Far Paired", "Nearby Unpaired"]);
    }

    #[test]
    fn discoverable_row_uses_selected_adapter_state() {
        let mut page = BluetoothPageState::from_service_state(service_state(
            vec![
                adapter("/org/bluez/hci0", "Built-in", true, true),
                adapter("/org/bluez/hci1", "USB", true, false),
            ],
            Vec::new(),
        ));
        page.select_adapter("/org/bluez/hci1");

        let adapter = page.selected_adapter().expect("selected adapter");

        assert!(!adapter.discoverable);
    }

    #[test]
    fn adapter_combo_is_hidden_when_only_one_adapter_exists() {
        let page = BluetoothPageState::from_service_state(service_state(
            vec![adapter("/org/bluez/hci0", "Built-in", true, true)],
            Vec::new(),
        ));

        assert!(!page.show_adapter_selector());
    }

    #[test]
    fn matches_applet_visibility_filtering_rules() {
        let page = BluetoothPageState::from_service_state(service_state(
            vec![adapter("/org/bluez/hci0", "Built-in", true, true)],
            vec![
                device(
                    "AA:BB:CC:DD:EE:01",
                    "",
                    "/org/bluez/hci0",
                    false,
                    false,
                    false,
                    Some(-30),
                ),
                device(
                    "AA:BB:CC:DD:EE:02",
                    "AA:BB:CC:DD:EE:02",
                    "/org/bluez/hci0",
                    false,
                    true,
                    false,
                    None,
                ),
                device(
                    "AA:BB:CC:DD:EE:03",
                    "Speaker",
                    "/org/bluez/hci0",
                    false,
                    false,
                    false,
                    Some(-40),
                ),
            ],
        ));

        let visible = page
            .visible_devices()
            .into_iter()
            .map(|device| device.address.as_str())
            .collect::<Vec<_>>();

        assert_eq!(visible, vec!["AA:BB:CC:DD:EE:02", "AA:BB:CC:DD:EE:03"]);
    }
}
