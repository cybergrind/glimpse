use std::{collections::HashMap, env, fs, path::PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelPosition {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Margin {
    #[serde(default)]
    pub left: i32,
    #[serde(default)]
    pub right: i32,
    #[serde(default)]
    pub top: i32,
    #[serde(default)]
    pub bottom: i32,
}

impl Default for Margin {
    fn default() -> Self {
        Self {
            left: 0,
            right: 0,
            top: 0,
            bottom: 0,
        }
    }
}

fn default_height() -> i32 {
    36
}

#[derive(Debug, Clone, Deserialize)]
pub struct PanelConfig {
    pub position: PanelPosition,
    #[serde(default = "default_height")]
    pub height: i32,
    #[serde(default)]
    pub margin: Margin,

    #[serde(default)]
    pub applets: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppletConfig {
    pub extends: String,
    #[serde(flatten)]
    pub settings: toml::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    path: Option<PathBuf>,

    #[serde(default)]
    pub panels: Vec<PanelConfig>,

    #[serde(default)]
    pub applets: HashMap<String, AppletConfig>,

    #[serde(default)]
    pub wallpaper: glimpse::wallpaper::WallpaperConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: None,
            applets: HashMap::new(),
            panels: vec![PanelConfig {
                height: 36,
                margin: Margin::default(),
                position: PanelPosition::Bottom,
                applets: vec![],
            }],
            wallpaper: glimpse::wallpaper::WallpaperConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = Self::config_path();
        if config_path.is_none() {
            tracing::warn!("no config file found");
            return Self::default();
        }

        let config_path = config_path.unwrap();
        tracing::info!("loading config from {}", config_path.display());

        if !config_path.is_file() {
            tracing::warn!("config file is not a file");
            return Self::default();
        }

        if let Ok(content) = fs::read_to_string(&config_path) {
            match toml::from_str::<Config>(&content) {
                Ok(mut config) => {
                    config.path = Some(config_path);
                    return config;
                }
                Err(e) => tracing::error!("failed to parse {}: {}", config_path.display(), e),
            }
        }

        Self {
            path: Some(config_path),
            ..Default::default()
        }
    }

    pub fn config_directory() -> PathBuf {
        env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(env::var("HOME").unwrap_or_default()).join(".config"))
            .join("glimpse")
    }

    pub fn theme_path(&self) -> PathBuf {
        Self::config_directory().join("theme.css")
    }

    fn config_path() -> Option<PathBuf> {
        let config_path = Self::config_directory().join("config.toml");
        tracing::debug!("possible config file {:?}", config_path);
        if config_path.exists() {
            Some(config_path)
        } else {
            None
        }
    }
}
