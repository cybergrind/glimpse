use std::{env, fs, path::PathBuf};

use serde::Deserialize;

use crate::services::location::LocationConfig;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub location: LocationConfig,
}

impl Config {
    pub fn autodetect() -> Self {
        if let Some(path) = Self::detect_from_env() {
            return Self::load_from_file(path);
        }

        if let Some(path) = Self::detect_from_dirs() {
            return Self::load_from_file(path);
        }
        Self::default()
    }

    fn load_from_file(path: PathBuf) -> Self {
        tracing::info!("loading configuration from {}", path.display());

        match fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<Self>(&content) {
                Ok(config) => config,
                Err(err) => {
                    tracing::error!("failed to parse config: {}", err);
                    Self::default()
                }
            },
            Err(err) => {
                tracing::error!("failed to read configuration file: {}", err);
                Self::default()
            }
        }
    }

    fn detect_from_env() -> Option<PathBuf> {
        if let Ok(path) = env::var("GLIMPSE_CONFIG") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    fn detect_from_dirs() -> Option<PathBuf> {
        let dirs = vec![
            PathBuf::from("config.toml"),
            env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    PathBuf::from(env::var("HOME").unwrap_or_default()).join(".config")
                })
                .join("glimpse")
                .join("config.toml"),
        ];

        let value = dirs
            .iter()
            .filter_map(|dir| dir.exists().then_some(dir.into()))
            .collect();
        value
    }
}
