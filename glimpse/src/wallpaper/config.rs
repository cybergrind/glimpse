use std::path::PathBuf;

use gtk4::ContentFit;
use serde::Deserialize;

fn default_transition_ms() -> u32 {
    800
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WallpaperMode {
    #[default]
    Color,
    Image,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ImageFit {
    Fill,
    Contain,
    #[default]
    Cover,
}

impl ImageFit {
    pub fn to_gtk(&self) -> ContentFit {
        match self {
            ImageFit::Fill => ContentFit::Fill,
            ImageFit::Contain => ContentFit::Contain,
            ImageFit::Cover => ContentFit::Cover,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WallpaperConfig {
    pub color: String,
    #[serde(default = "default_transition_ms")]
    pub transition_ms: u32,
    pub mode: WallpaperMode,
    pub path: Option<PathBuf>,
    pub fit: ImageFit,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            color: "transparent".to_owned(),
            transition_ms: default_transition_ms(),
            mode: WallpaperMode::default(),
            path: None,
            fit: ImageFit::default(),
        }
    }
}
