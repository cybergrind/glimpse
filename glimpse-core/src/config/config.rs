use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};
use tokio::sync::mpsc;

use crate::{
    AppletConfig, BackdropConfig, ConfigFileDiscovery, IdleConfig, KeyboardConfig, LocationConfig,
    LockConfig, NightLightConfig, PanelConfig, ResolvedWallpaperSpec, ThemeMode, WallpaperConfig,
    resolve_wallpaper_spec, watch_config_file,
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub location: LocationConfig,
    pub theme: String,
    pub theme_mode: ThemeMode,
    pub panels: Vec<PanelConfig>,
    pub applets: HashMap<String, AppletConfig>,
    #[serde(default)]
    pub night_light: NightLightConfig,
    #[serde(default)]
    pub idle: IdleConfig,
    #[serde(default)]
    pub keyboard: KeyboardConfig,
    #[serde(default)]
    pub wallpaper: WallpaperConfig,
    #[serde(default)]
    pub backdrop: BackdropConfig,
    #[serde(default)]
    pub lock: LockConfig,
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
        let mut config = toml::from_str::<Self>(content)?;
        config.expand_panel_placeholders();
        Ok(config)
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
            .unwrap_or_else(|_| Self::themes_dir().join(format!("{}.css", self.theme)))
    }

    pub fn resolve_wallpaper(&self) -> ResolvedWallpaperSpec {
        resolve_wallpaper_spec(&self.wallpaper, &self.backdrop)
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

    fn expand_panel_placeholders(&mut self) {
        let defaults = PanelConfig::default();
        for panel in &mut self.panels {
            expand_panel_section("left", &mut panel.left, &defaults.left);
            expand_panel_section("center", &mut panel.center, &defaults.center);
            expand_panel_section("right", &mut panel.right, &defaults.right);
        }
    }
}

fn expand_panel_section(section: &'static str, applets: &mut Vec<String>, defaults: &[String]) {
    let mut expanded = Vec::with_capacity(applets.len() + defaults.len());
    let mut inserted_defaults = false;
    for applet in applets.drain(..) {
        if applet == crate::DEFAULT_PANEL_APPLETS_PLACEHOLDER {
            if inserted_defaults {
                tracing::warn!(
                    section,
                    placeholder = crate::DEFAULT_PANEL_APPLETS_PLACEHOLDER,
                    "extra panel applet placeholder ignored"
                );
                continue;
            }
            expanded.extend(defaults.iter().cloned());
            inserted_defaults = true;
            continue;
        }
        expanded.push(applet);
    }
    *applets = expanded;
}

impl Default for Config {
    fn default() -> Self {
        Self {
            location: LocationConfig::default(),
            theme: "adwaita".into(),
            theme_mode: ThemeMode::Auto,
            panels: vec![PanelConfig::default()],
            applets: HashMap::new(),
            night_light: NightLightConfig::default(),
            idle: IdleConfig::default(),
            keyboard: KeyboardConfig::default(),
            wallpaper: WallpaperConfig::default(),
            backdrop: BackdropConfig::default(),
            lock: LockConfig::default(),
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

#[cfg(test)]
mod panel_config_tests {
    use crate::{Config, PanelConfig};

    #[test]
    fn config_expands_panel_default_placeholder() {
        let config = Config::from_toml_str(
            r#"
[[panels]]
left = ["custom", "..."]
center = ["..."]
right = ["...", "custom"]
"#,
        )
        .unwrap();

        assert_eq!(config.panels[0].left, vec!["custom", "pager", "mpris"]);
        assert_eq!(
            config.panels[0].center,
            vec!["clock", "weather", "notifications"]
        );
        assert_eq!(
            config.panels[0].right,
            vec![
                "tray",
                "removable",
                "clipboard",
                "keyboard",
                "privacy",
                "bluetooth",
                "network",
                "brightness",
                "audio",
                "battery",
                "session",
                "custom"
            ]
        );
    }

    #[test]
    fn config_keeps_panel_section_without_placeholder_as_full_override() {
        let config = Config::from_toml_str(
            r#"
[[panels]]
right = ["custom"]
"#,
        )
        .unwrap();

        assert_eq!(config.panels[0].right, vec!["custom"]);
    }

    #[test]
    fn config_expands_only_first_panel_default_placeholder() {
        let config = Config::from_toml_str(
            r#"
[[panels]]
center = ["before", "...", "middle", "...", "after"]
"#,
        )
        .unwrap();

        assert_eq!(
            config.panels[0].center,
            vec![
                "before",
                "clock",
                "weather",
                "notifications",
                "middle",
                "after"
            ]
        );
    }

    #[test]
    fn panel_config_deserialize_keeps_placeholder_until_config_normalization() {
        let panel = toml::from_str::<PanelConfig>(
            r#"
left = ["custom", "..."]
"#,
        )
        .unwrap();

        assert_eq!(panel.left, vec!["custom", "..."]);
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
