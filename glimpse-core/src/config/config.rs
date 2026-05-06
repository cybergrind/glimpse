use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};
use tokio::sync::mpsc;

use crate::{
    AppletConfig, BackdropConfig, BackgroundSettings, ConfigFileDiscovery, IdleConfig,
    LocationConfig, NightLightConfig, PanelConfig, ThemeConfig, ThemeMode, WallpaperConfig,
    wallpaper_spec, watch_config_file,
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
    #[serde(default)]
    pub night_light: NightLightConfig,
    #[serde(default)]
    pub idle: IdleConfig,
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
        match Self::try_load_from_file(path) {
            Ok(config) => config,
            Err(err) => {
                tracing::error!("failed to load configuration: {}", err);
                Self::default()
            }
        }
    }

    pub fn try_load_from_file(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(content) => match Self::from_toml_str(&content) {
                Ok(config) => Ok(config),
                Err(err) => Err(format!("failed to parse config: {err}")),
            },
            Err(err) => Err(format!("failed to read configuration file: {err}")),
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
            night_light: NightLightConfig::default(),
            idle: IdleConfig::default(),
        }
    }
}

#[cfg(test)]
mod night_light_config_tests {
    use crate::{Config, NightLightSchedule};

    #[test]
    fn default_config_includes_disabled_night_light() {
        let config = Config::default();

        assert_eq!(config.night_light.temperature, 4200);
        assert_eq!(config.night_light.schedule, NightLightSchedule::Off);
        assert_eq!(config.night_light.transition_minutes, 15);
    }

    #[test]
    fn config_parses_night_light_block() {
        let config = Config::from_toml_str(
            r#"
[night_light]
temperature = 4200
schedule = "schedule"
start_time = "18:00"
end_time = "06:30"
transition_minutes = 75
"#,
        )
        .expect("config should parse");

        assert_eq!(config.night_light.temperature, 4200);
        assert_eq!(config.night_light.schedule, NightLightSchedule::Schedule);
        assert_eq!(config.night_light.start_time.as_deref(), Some("18:00"));
        assert_eq!(config.night_light.end_time.as_deref(), Some("06:30"));
        assert_eq!(config.night_light.transition_minutes, 75);
    }
}

#[cfg(test)]
mod idle_config_tests {
    use crate::Config;

    #[test]
    fn default_config_includes_idle_without_listener_policies() {
        let config = Config::default();

        assert!(config.idle.enabled);
        assert!(config.idle.respect_inhibitors);
        assert!(config.idle.profiles.ac.listeners.is_empty());
        assert!(config.idle.profiles.battery.listeners.is_empty());
    }

    #[test]
    fn config_parses_idle_block() {
        let config = Config::from_toml_str(
            r#"
[idle]
enabled = true
respect_inhibitors = false

[idle.profiles.ac]
listeners = [
  { timeout = 60, on_idle = "one", on_resume = "two", respect_inhibitors = true },
]

[idle.profiles.battery]
listeners = [
  { timeout = 30, on_idle = "three" },
]
"#,
        )
        .expect("config should parse");

        assert!(!config.idle.respect_inhibitors);
        assert_eq!(config.idle.profiles.ac.listeners[0].timeout, 60);
        assert_eq!(
            config.idle.profiles.ac.listeners[0].respect_inhibitors,
            Some(true)
        );
    }
}

#[derive(Debug, Clone)]
pub struct ConfigDiscovery {
    inner: ConfigFileDiscovery,
}

impl ConfigDiscovery {
    pub fn new(
        env: HashMap<String, String>,
        cwd: PathBuf,
        xdg_config_home: Option<PathBuf>,
        home: Option<PathBuf>,
    ) -> Self {
        Self {
            inner: ConfigFileDiscovery::new(
                env,
                cwd,
                xdg_config_home,
                home,
                "GLIMPSE_CONFIG",
                "config.toml",
            ),
        }
    }

    pub fn from_process() -> Self {
        Self {
            inner: ConfigFileDiscovery::from_process("GLIMPSE_CONFIG", "config.toml"),
        }
    }

    pub fn detect_config_file(&self) -> PathBuf {
        self.inner.detect_config_file()
    }

    pub fn config_dir(&self) -> PathBuf {
        self.inner.config_dir()
    }

    pub fn config_file(&self) -> PathBuf {
        self.inner.config_file()
    }
}

pub enum ConfigEvent {
    Changed(Config),
}

pub async fn watch_for_config_changes(sender: mpsc::Sender<ConfigEvent>) {
    watch_config_file(Config::detect_config_file(), sender, "shared", |path| {
        ConfigEvent::Changed(Config::load_from_file(path))
    })
    .await;
}
