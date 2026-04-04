use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BatteryConfig {
    pub show_icon: bool,
    /// Label format when on battery. Empty = no label.
    /// Keys: {percentage}, {state}, {time_left}, {power}, {health}
    pub label_on_battery: String,
    /// Label format when on AC.
    pub label_on_ac: String,
    /// Tooltip when on battery.
    pub tooltip_on_battery: String,
    /// Tooltip when on AC.
    pub tooltip_on_ac: String,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            show_icon: true,
            label_on_battery: "{percentage}%".into(),
            label_on_ac: String::new(),
            tooltip_on_battery: "{percentage}% {state}, {time_left}".into(),
            tooltip_on_ac: "{percentage}% {state}".into(),
        }
    }
}
