use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_wallpaper_color() -> String {
    "#101010".into()
}

fn default_transition_ms() -> u32 {
    800
}

fn default_blur_radius() -> u32 {
    24
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
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
    pub backdrop: ResolvedBackdropSpec,
}

pub fn resolve_wallpaper_spec(
    wallpaper: &WallpaperConfig,
    backdrop: &BackdropConfig,
) -> ResolvedWallpaperSpec {
    let backdrop_path = backdrop.path.clone().or_else(|| wallpaper.path.clone());
    ResolvedWallpaperSpec {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wallpaper_config() {
        let config: crate::Config = toml::from_str(
            r##"
[wallpaper]
color = "#203040"
path = "/tmp/wall.png"
fit = "contain"
transition_ms = 250

[backdrop]
enabled = true
path = "/tmp/backdrop.png"
blur_radius = 18
"##,
        )
        .unwrap();

        assert_eq!(config.wallpaper.color, "#203040");
        assert_eq!(config.wallpaper.path, Some(PathBuf::from("/tmp/wall.png")));
        assert_eq!(config.wallpaper.fit, FitMode::Contain);
        assert_eq!(config.wallpaper.transition_ms, 250);
        assert_eq!(
            config.backdrop.path,
            Some(PathBuf::from("/tmp/backdrop.png"))
        );
        assert_eq!(config.backdrop.blur_radius, 18);
    }

    #[test]
    fn resolves_color_only_wallpaper_spec() {
        let config = crate::Config {
            wallpaper: WallpaperConfig {
                color: "#101010".into(),
                path: None,
                fit: FitMode::Cover,
                transition_ms: 800,
            },
            backdrop: BackdropConfig {
                enabled: false,
                ..BackdropConfig::default()
            },
            ..crate::Config::default()
        };

        assert_eq!(
            config.resolve_wallpaper(),
            ResolvedWallpaperSpec {
                color: "#101010".into(),
                image: None,
                transition_ms: 800,
                backdrop: ResolvedBackdropSpec::Disabled,
            }
        );
    }

    #[test]
    fn resolves_wallpaper_and_backdrop_image_spec() {
        let config = crate::Config {
            wallpaper: WallpaperConfig {
                color: "#202020".into(),
                path: Some(PathBuf::from("/tmp/wall.png")),
                fit: FitMode::Fill,
                transition_ms: 100,
            },
            backdrop: BackdropConfig {
                enabled: true,
                path: Some(PathBuf::from("/tmp/backdrop.png")),
                blur_radius: 24,
            },
            ..crate::Config::default()
        };

        assert_eq!(
            config.resolve_wallpaper(),
            ResolvedWallpaperSpec {
                color: "#202020".into(),
                image: Some(ResolvedImageSpec {
                    path: PathBuf::from("/tmp/wall.png"),
                    fit: FitMode::Fill,
                }),
                transition_ms: 100,
                backdrop: ResolvedBackdropSpec::Enabled {
                    path: Some(PathBuf::from("/tmp/backdrop.png")),
                    blur_radius: 24,
                },
            }
        );
    }

    #[test]
    fn enabled_backdrop_without_path_falls_back_to_wallpaper_image() {
        let config = crate::Config {
            wallpaper: WallpaperConfig {
                color: "#202020".into(),
                path: Some(PathBuf::from("/tmp/wall.png")),
                fit: FitMode::Cover,
                transition_ms: 800,
            },
            backdrop: BackdropConfig {
                enabled: true,
                path: None,
                blur_radius: 24,
            },
            ..crate::Config::default()
        };

        assert_eq!(
            config.resolve_wallpaper().backdrop,
            ResolvedBackdropSpec::Enabled {
                path: Some(PathBuf::from("/tmp/wall.png")),
                blur_radius: 24,
            }
        );
    }
}
