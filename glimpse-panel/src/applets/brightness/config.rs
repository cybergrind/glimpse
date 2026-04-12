use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BrightnessConfig {
    pub show_icon: bool,
    pub label_format: String,
    pub scroll_step: u32,
    pub hide_when_unavailable: bool,
    pub settings_command: String,
}

impl Default for BrightnessConfig {
    fn default() -> Self {
        Self {
            show_icon: true,
            label_format: String::new(),
            scroll_step: 5,
            hide_when_unavailable: true,
            settings_command: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BrightnessConfig;

    #[test]
    fn default_brightness_config_shows_icon_and_internal_label() {
        let config = BrightnessConfig::default();
        assert!(config.show_icon);
        assert_eq!(config.label_format, "");
        assert_eq!(config.scroll_step, 5);
    }
}
