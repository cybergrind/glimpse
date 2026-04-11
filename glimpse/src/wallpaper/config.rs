use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WallpaperConfig {
    pub color: String,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            color: "transparent".to_owned(),
        }
    }
}
