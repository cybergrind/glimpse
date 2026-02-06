use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct TimezoneEntry {
    pub name: String,
    pub timezone: String,

    #[serde(default = "default_format")]
    pub format: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClockConfig {
    #[serde(default = "default_format")]
    pub format: String,
    #[serde(default)]
    pub timezones: Vec<TimezoneEntry>,
}
impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
            timezones: Vec::new(),
        }
    }
}

fn default_format() -> String {
    "%H:%M".to_string()
}
