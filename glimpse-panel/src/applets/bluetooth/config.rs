use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BluetoothConfig {
    pub settings_command: String,
    pub scan_interval: u64,
}

impl Default for BluetoothConfig {
    fn default() -> Self {
        Self {
            settings_command: "blueman-manager".into(),
            scan_interval: 15,
        }
    }
}
