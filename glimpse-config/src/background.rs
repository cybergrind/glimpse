use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::ThemeMode;

fn default_wallpaper_color() -> String {
    "#101010".into()
}

fn default_transition_ms() -> u32 {
    800
}

fn default_blur_radius() -> u32 {
    24
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FitMode {
    #[default]
    Cover,
    Contain,
    Fill,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct WallpaperConfig {
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub fit: FitMode,
    pub transition_ms: u32,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            color: default_wallpaper_color(),
            path: None,
            fit: FitMode::Cover,
            transition_ms: default_transition_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct BackdropConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub blur_radius: u32,
}

impl Default for BackdropConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: None,
            blur_radius: default_blur_radius(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedImageSpec {
    pub path: PathBuf,
    pub fit: FitMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBackdropSpec {
    Disabled,
    Enabled {
        path: Option<PathBuf>,
        blur_radius: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWallpaperSpec {
    pub color: String,
    pub image: Option<ResolvedImageSpec>,
    pub transition_ms: u32,
    pub theme_mode: ThemeMode,
    pub backdrop: ResolvedBackdropSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperSpec {
    pub color: String,
    pub image: Option<ResolvedImageSpec>,
    pub transition_ms: u32,
    pub backdrop: ResolvedBackdropSpec,
}

impl WallpaperSpec {
    pub fn resolve(self, theme_mode: ThemeMode) -> ResolvedWallpaperSpec {
        ResolvedWallpaperSpec {
            color: self.color,
            image: self.image,
            transition_ms: self.transition_ms,
            theme_mode,
            backdrop: self.backdrop,
        }
    }
}

pub fn wallpaper_spec(wallpaper: &WallpaperConfig, backdrop: &BackdropConfig) -> WallpaperSpec {
    let backdrop_path = backdrop.path.clone().or_else(|| wallpaper.path.clone());
    WallpaperSpec {
        color: wallpaper.color.clone(),
        image: wallpaper.path.clone().map(|path| ResolvedImageSpec {
            path,
            fit: wallpaper.fit,
        }),
        transition_ms: wallpaper.transition_ms,
        backdrop: if backdrop.enabled {
            if let Some(path) = backdrop_path {
                ResolvedBackdropSpec::Enabled {
                    path: Some(path),
                    blur_radius: backdrop.blur_radius,
                }
            } else {
                ResolvedBackdropSpec::Disabled
            }
        } else {
            ResolvedBackdropSpec::Disabled
        },
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundSettings<'a> {
    pub wallpaper: &'a WallpaperConfig,
    pub backdrop: &'a BackdropConfig,
}
