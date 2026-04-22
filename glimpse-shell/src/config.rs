use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::sync::mpsc;

use crate::{panels, services::location::LocationConfig, theme::ThemeConfig};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Config {
    pub location: LocationConfig,
    pub theme: ThemeConfig,
    pub panels: Vec<panels::Config>,
}

impl Config {
    pub fn autodetect() -> Self {
        let path = Self::detect_config_file();
        if path.exists() && path.is_file() {
            return Self::load_from_file(&path);
        }
        Self::default()
    }

    pub fn detect_config_file() -> PathBuf {
        if let Some(path) = Self::detect_from_env() {
            return path;
        }

        if let Some(path) = Self::detect_from_dirs() {
            return path;
        }
        Self::config_file()
    }

    fn load_from_file(path: &Path) -> Self {
        tracing::info!("loading configuration from {}", path.display());

        match fs::read_to_string(path) {
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

    pub fn config_dir() -> PathBuf {
        env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(env::var("HOME").unwrap_or_default()).join(".config"))
            .join("glimpse")
    }

    pub fn config_file() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    fn detect_from_dirs() -> Option<PathBuf> {
        let dirs = vec![PathBuf::from("config.toml"), Config::config_file()];
        dirs.into_iter().find(|p| p.exists())
    }

    pub fn theme_file(&self) -> PathBuf {
        env::var("GLIMPSE_THEME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                Self::config_dir()
                    .join("theme")
                    .join(self.theme.name.clone())
            })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: ThemeConfig::default(),
            location: LocationConfig::default(),
            panels: vec![panels::Config::default()],
        }
    }
}

pub enum ConfigEvent {
    Changed(Config),
}

pub async fn watch_for_config_changes(sender: mpsc::Sender<ConfigEvent>) {
    let config_file = match Config::detect_config_file().canonicalize() {
        Ok(path) => path,
        Err(err) => {
            tracing::error!("cannot get canonical config file name: {}", err);
            return;
        }
    };
    let Some(config_dir) = config_file.parent().map(PathBuf::from) else {
        tracing::error!(
            "config file has no parent directory: {}",
            config_file.display()
        );
        return;
    };

    tracing::info!(
        "watching config file for changes: {}",
        config_file.display()
    );

    let handler_file = config_file.clone();
    let handler_sender = sender.clone();
    let mut debouncer = match new_debouncer(
        Duration::from_millis(200),
        None,
        move |res: DebounceEventResult| {
            file_change_handler(res, handler_file.clone(), handler_sender.clone());
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("failed to create watcher: {}", e);
            return;
        }
    };

    if let Err(e) = debouncer.watch(&config_dir, notify::RecursiveMode::Recursive) {
        tracing::error!("failed to watch config directory: {}", e);
        return;
    }

    sender.closed().await;
}

fn file_change_handler(
    res: DebounceEventResult,
    config_file: PathBuf,
    sender: mpsc::Sender<ConfigEvent>,
) {
    let events = match res {
        Ok(events) => events,
        Err(_) => return,
    };

    for event in events {
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                if event.paths.contains(&config_file) {
                    let config = Config::load_from_file(&config_file);
                    if let Err(e) = sender.try_send(ConfigEvent::Changed(config)) {
                        tracing::error!("failed to broadcast config change to the app: {}", e);
                    }
                }
            }
            _ => {}
        }
    }
}
