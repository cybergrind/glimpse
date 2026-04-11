use serde::Deserialize;

fn default_transition_ms() -> u32 {
    800
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WallpaperConfig {
    pub color: String,
    #[serde(default = "default_transition_ms")]
    pub transition_ms: u32,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            color: "transparent".to_owned(),
            transition_ms: default_transition_ms(),
        }
    }
}
