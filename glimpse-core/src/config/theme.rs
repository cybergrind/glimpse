use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    Auto,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub mode: ThemeMode,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "adwaita".into(),
            mode: ThemeMode::Auto,
        }
    }
}
