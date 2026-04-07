use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardFormat {
    Short,
    Full,
}

fn default_format() -> KeyboardFormat {
    KeyboardFormat::Short
}

fn default_per_window() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct KeyboardConfig {
    #[serde(default = "default_format")]
    pub format: KeyboardFormat,
    #[serde(default = "default_per_window")]
    pub per_window: bool,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
            per_window: default_per_window(),
            labels: HashMap::new(),
        }
    }
}
