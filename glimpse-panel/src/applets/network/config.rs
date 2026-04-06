use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    pub label_format: String,
    pub tooltip_format: String,
    pub show_vpn_icon: bool,
    pub settings_command: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            label_format: String::new(),
            tooltip_format: String::new(),
            show_vpn_icon: true,
            settings_command: "nm-connection-editor".into(),
        }
    }
}
