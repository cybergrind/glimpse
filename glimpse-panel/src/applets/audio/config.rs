use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub show_icon: bool,
    /// Label format. Empty = no label. Keys: {volume}, {device}
    pub label_format: String,
    /// Tooltip format. Empty = no tooltip. Keys: {volume}, {device}
    pub tooltip_format: String,
    /// Volume step for scroll (percentage points).
    pub scroll_step: u32,
    /// Max volume (100 = no overamplification, 150 = allow overamplification).
    pub max_volume: u32,
    /// Command to open audio settings. Empty = hide the button.
    pub settings_command: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            show_icon: true,
            label_format: String::new(),
            tooltip_format: "{device} — {volume}%".into(),
            scroll_step: 5,
            max_volume: 100,
            settings_command: "pavucontrol".into(),
        }
    }
}
