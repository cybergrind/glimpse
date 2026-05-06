use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use tokio::sync::mpsc;

use glimpse_core::watch_config_file;

const CONFIG_ENV: &str = "GLIMPSE_WALLPAPER_CONFIG";
const CONFIG_FILE_NAME: &str = "wallpaper.toml";
pub const EXPORTED_CONFIG: &str = include_str!("../resources/wallpaper.toml");

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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub wallpaper: WallpaperConfig,
    pub backdrop: BackdropConfig,
}

impl Config {
    pub fn load() -> Self {
        let path = Self::detect_config_file();
        if path.exists() && path.is_file() {
            return Self::load_from_file(&path);
        }
        Self::default()
    }

    pub fn from_toml_str(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str::<Self>(content)
    }

    pub fn detect_config_file() -> PathBuf {
        config_file_from_env(
            std::env::vars().collect(),
            dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")),
        )
    }

    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("glimpse")
    }

    pub fn config_file() -> PathBuf {
        Self::config_dir().join(CONFIG_FILE_NAME)
    }

    pub fn load_from_file(path: &Path) -> Self {
        tracing::info!("loading wallpaper configuration from {}", path.display());
        match Self::try_load_from_file(path) {
            Ok(config) => config,
            Err(err) => {
                tracing::error!("failed to load wallpaper configuration: {err}");
                Self::default()
            }
        }
    }

    pub fn try_load_from_file(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(content) => match Self::from_toml_str(&content) {
                Ok(config) => Ok(config),
                Err(err) => Err(format!("failed to parse wallpaper config: {err}")),
            },
            Err(err) => Err(format!(
                "failed to read wallpaper configuration file: {err}"
            )),
        }
    }

    pub fn resolve_wallpaper(&self) -> ResolvedWallpaperSpec {
        wallpaper_spec(&self.wallpaper, &self.backdrop)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            wallpaper: WallpaperConfig::default(),
            backdrop: BackdropConfig::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConfigEvent {
    Changed(Config),
}

pub async fn watch_for_config_changes(sender: mpsc::Sender<ConfigEvent>) {
    watch_config_file(Config::detect_config_file(), sender, "wallpaper", |path| {
        ConfigEvent::Changed(Config::load_from_file(path))
    })
    .await;
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

fn wallpaper_spec(wallpaper: &WallpaperConfig, backdrop: &BackdropConfig) -> ResolvedWallpaperSpec {
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

fn config_file_from_env(env: HashMap<String, String>, xdg_config_home: PathBuf) -> PathBuf {
    env.get(CONFIG_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| config_file_from_config_home(xdg_config_home))
}

fn config_file_from_config_home(xdg_config_home: PathBuf) -> PathBuf {
    xdg_config_home.join("glimpse").join(CONFIG_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallpaper_config_file_prefers_app_specific_env() {
        let path = config_file_from_env(
            HashMap::from([(CONFIG_ENV.into(), "/tmp/custom-wallpaper.toml".into())]),
            PathBuf::from("/tmp/config"),
        );

        assert_eq!(path, PathBuf::from("/tmp/custom-wallpaper.toml"));
    }

    #[test]
    fn wallpaper_config_file_defaults_to_xdg_glimpse_wallpaper_toml() {
        let path = config_file_from_env(HashMap::new(), PathBuf::from("/tmp/config"));

        assert_eq!(path, PathBuf::from("/tmp/config/glimpse/wallpaper.toml"));
    }

    #[test]
    fn embedded_export_config_parses() {
        let config = Config::from_toml_str(EXPORTED_CONFIG).unwrap();

        assert_eq!(config.wallpaper.fit, FitMode::Cover);
        assert!(config.backdrop.enabled);
    }

    #[test]
    fn parses_wallpaper_config() {
        let config = Config::from_toml_str(
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
        let config = Config {
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
        let config = Config {
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
        let config = Config {
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
        };

        assert_eq!(
            config.resolve_wallpaper().backdrop,
            ResolvedBackdropSpec::Enabled {
                path: Some(PathBuf::from("/tmp/wall.png")),
                blur_radius: 24,
            }
        );
    }

    #[test]
    fn backdrop_defaults_to_enabled_with_blur_24_and_wallpaper_fallback() {
        let config = Config {
            wallpaper: WallpaperConfig {
                color: "#202020".into(),
                path: Some(PathBuf::from("/tmp/wall.png")),
                fit: FitMode::Cover,
                transition_ms: 800,
            },
            ..Config::default()
        };

        assert_eq!(
            config.backdrop,
            BackdropConfig {
                enabled: true,
                path: None,
                blur_radius: 24,
            }
        );
        assert_eq!(
            config.resolve_wallpaper().backdrop,
            ResolvedBackdropSpec::Enabled {
                path: Some(PathBuf::from("/tmp/wall.png")),
                blur_radius: 24,
            }
        );
    }
}
