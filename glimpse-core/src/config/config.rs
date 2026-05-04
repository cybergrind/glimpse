use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::sync::mpsc;

use crate::{
    AppletConfig, BackdropConfig, BackgroundSettings, LocationConfig, PanelConfig, ThemeConfig,
    ThemeMode, WallpaperConfig, wallpaper_spec,
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub location: LocationConfig,
    pub theme: ThemeConfig,
    pub panels: Vec<PanelConfig>,
    pub applets: HashMap<String, AppletConfig>,
    pub wallpaper: WallpaperConfig,
    pub backdrop: BackdropConfig,
}

impl Config {
    pub fn autodetect() -> Self {
        Self::load()
    }

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
        ConfigDiscovery::from_process().detect_config_file()
    }

    pub fn config_dir() -> PathBuf {
        ConfigDiscovery::from_process().config_dir()
    }

    pub fn config_file() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn themes_dir() -> PathBuf {
        Self::config_dir().join("themes")
    }

    pub fn theme_file(&self) -> PathBuf {
        env::var("GLIMPSE_THEME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| Self::themes_dir().join(format!("{}.css", self.theme.name)))
    }

    pub fn load_from_file(path: &Path) -> Self {
        tracing::info!("loading configuration from {}", path.display());
        match fs::read_to_string(path) {
            Ok(content) => match Self::from_toml_str(&content) {
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

    pub fn resolve_wallpaper(&self, theme_mode: ThemeMode) -> crate::ResolvedWallpaperSpec {
        wallpaper_spec(&self.wallpaper, &self.backdrop).resolve(theme_mode)
    }

    pub fn background_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(&BackgroundSettings {
            wallpaper: &self.wallpaper,
            backdrop: &self.backdrop,
        })
    }

    pub fn persist_background_settings(
        mut self,
        wallpaper: WallpaperConfig,
        backdrop: BackdropConfig,
    ) -> Result<PathBuf, String> {
        self.wallpaper = wallpaper;
        self.backdrop = backdrop;
        let path = Self::config_file();
        let parent = path
            .parent()
            .ok_or_else(|| "config path has no parent directory".to_string())?;
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        let content = toml::to_string_pretty(&self).map_err(|err| err.to_string())?;
        fs::write(&path, content).map_err(|err| err.to_string())?;
        Ok(path)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            location: LocationConfig::default(),
            theme: ThemeConfig::default(),
            panels: vec![PanelConfig::default()],
            applets: HashMap::new(),
            wallpaper: WallpaperConfig::default(),
            backdrop: BackdropConfig::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigDiscovery {
    env: HashMap<String, String>,
    cwd: PathBuf,
    xdg_config_home: Option<PathBuf>,
    home: Option<PathBuf>,
}

impl ConfigDiscovery {
    pub fn new(
        env: HashMap<String, String>,
        cwd: PathBuf,
        xdg_config_home: Option<PathBuf>,
        home: Option<PathBuf>,
    ) -> Self {
        Self {
            env,
            cwd,
            xdg_config_home,
            home,
        }
    }

    pub fn from_process() -> Self {
        Self {
            env: env::vars().collect(),
            cwd: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            xdg_config_home: env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            home: env::var_os("HOME").map(PathBuf::from),
        }
    }

    pub fn detect_config_file(&self) -> PathBuf {
        if let Some(path) = self.detect_from_env() {
            return path;
        }
        if let Some(path) = self.detect_from_dirs() {
            return path;
        }
        self.config_file()
    }

    pub fn config_dir(&self) -> PathBuf {
        self.xdg_config_home
            .clone()
            .or_else(|| self.home.clone().map(|home| home.join(".config")))
            .unwrap_or_default()
            .join("glimpse")
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir().join("config.toml")
    }

    fn detect_from_env(&self) -> Option<PathBuf> {
        self.env
            .get("GLIMPSE_CONFIG")
            .map(PathBuf::from)
            .filter(|path| path.exists())
    }

    fn detect_from_dirs(&self) -> Option<PathBuf> {
        [self.cwd.join("config.toml"), self.config_file()]
            .into_iter()
            .find(|path| path.exists())
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
        Ok(debouncer) => debouncer,
        Err(err) => {
            tracing::error!("failed to create watcher: {}", err);
            return;
        }
    };

    if let Err(err) = debouncer.watch(&config_dir, notify::RecursiveMode::Recursive) {
        tracing::error!("failed to watch config directory: {}", err);
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
                    if let Err(err) = sender.try_send(ConfigEvent::Changed(config)) {
                        tracing::error!("failed to broadcast config change to the app: {}", err);
                    }
                }
            }
            _ => {}
        }
    }
}
