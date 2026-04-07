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

#[derive(Debug, Clone, Deserialize)]
pub struct KeyboardConfig {
    #[serde(default = "default_format")]
    pub format: KeyboardFormat,
    #[serde(default)]
    pub per_window: bool,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
            per_window: false,
        }
    }
}
