use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct TrayConfig {
    #[serde(default = "default_icon_size")]
    pub icon_size: i32,
}

fn default_icon_size() -> i32 {
    16
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self { icon_size: 16 }
    }
}
