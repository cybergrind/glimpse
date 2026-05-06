use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::sync::mpsc;

use glimpse_core::{FitMode, ResolvedImageSpec, WallpaperConfig};
use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};

fn default_css_path() -> String {
    "themes/lock.css".into()
}

fn default_pam_service() -> String {
    "glimpse-lock".into()
}

fn default_dim() -> f32 {
    0.35
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct LockConfig {
    pub pam_service: String,
    pub css_path: String,
    pub background: LockBackgroundConfig,
    pub clock: LockClockConfig,
    pub controls: LockControlsConfig,
}

impl LockConfig {
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
        LockConfigDiscovery::from_process().detect_config_file()
    }

    pub fn config_dir() -> PathBuf {
        LockConfigDiscovery::from_process().config_dir()
    }

    pub fn config_file() -> PathBuf {
        Self::config_dir().join("lock.toml")
    }

    pub fn load_from_file(path: &Path) -> Self {
        tracing::info!("loading lock configuration from {}", path.display());
        match fs::read_to_string(path) {
            Ok(content) => match Self::from_toml_str(&content) {
                Ok(config) => config,
                Err(err) => {
                    tracing::error!("failed to parse lock config: {}", err);
                    Self::default()
                }
            },
            Err(err) => {
                tracing::error!("failed to read lock configuration file: {}", err);
                Self::default()
            }
        }
    }
}

impl Default for LockConfig {
    fn default() -> Self {
        Self {
            pam_service: default_pam_service(),
            css_path: default_css_path(),
            background: LockBackgroundConfig::default(),
            clock: LockClockConfig::default(),
            controls: LockControlsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct LockBackgroundConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<FitMode>,
    pub blur_radius: u32,
    pub dim: f32,
}

impl Default for LockBackgroundConfig {
    fn default() -> Self {
        Self {
            color: None,
            path: None,
            fit: None,
            blur_radius: 0,
            dim: default_dim(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LockControlButton {
    Weather,
    Wifi,
    Input,
    Battery,
    Power,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct LockControlsConfig {
    pub buttons: Vec<LockControlButton>,
}

impl Default for LockControlsConfig {
    fn default() -> Self {
        Self {
            buttons: vec![
                LockControlButton::Weather,
                LockControlButton::Wifi,
                LockControlButton::Input,
                LockControlButton::Battery,
                LockControlButton::Power,
            ],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct LockClockConfig {
    pub enabled: bool,
    pub time_format: String,
    pub date_format: String,
}

impl Default for LockClockConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            time_format: "%H:%M".into(),
            date_format: "%A, %B %-d".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLockSpec {
    pub pam_service: String,
    pub css_path: PathBuf,
    pub background: ResolvedLockBackgroundSpec,
    pub clock: ResolvedLockClockSpec,
    pub controls: Vec<LockControlButton>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLockBackgroundSpec {
    pub color: String,
    pub image: Option<ResolvedImageSpec>,
    pub blur_radius: u32,
    pub dim: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLockClockSpec {
    pub enabled: bool,
    pub time_format: String,
    pub date_format: String,
}

pub fn resolve_lock_spec(
    lock: &LockConfig,
    wallpaper: &WallpaperConfig,
    config_dir: &Path,
) -> ResolvedLockSpec {
    let color = lock
        .background
        .color
        .clone()
        .unwrap_or_else(|| wallpaper.color.clone());
    let has_lock_image = lock.background.path.is_some();
    let path = lock
        .background
        .path
        .clone()
        .or_else(|| wallpaper.path.clone());
    let fit = lock.background.fit.unwrap_or(if has_lock_image {
        FitMode::Cover
    } else {
        wallpaper.fit
    });
    let css_path = PathBuf::from(&lock.css_path);

    ResolvedLockSpec {
        pam_service: lock.pam_service.clone(),
        css_path: if css_path.is_absolute() {
            css_path
        } else {
            config_dir.join(css_path)
        },
        background: ResolvedLockBackgroundSpec {
            color,
            image: path.map(|path| ResolvedImageSpec { path, fit }),
            blur_radius: lock.background.blur_radius,
            dim: lock.background.dim.clamp(0.0, 1.0),
        },
        clock: ResolvedLockClockSpec {
            enabled: lock.clock.enabled,
            time_format: lock.clock.time_format.clone(),
            date_format: lock.clock.date_format.clone(),
        },
        controls: lock.controls.buttons.clone(),
    }
}

#[derive(Debug, Clone)]
pub struct LockConfigDiscovery {
    env: HashMap<String, String>,
    cwd: PathBuf,
    xdg_config_home: Option<PathBuf>,
    home: Option<PathBuf>,
}

impl LockConfigDiscovery {
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
            env: std::env::vars().collect(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            home: std::env::var_os("HOME").map(PathBuf::from),
        }
    }

    pub fn detect_config_file(&self) -> PathBuf {
        if let Some(path) = self
            .env
            .get("GLIMPSE_LOCK_CONFIG")
            .map(PathBuf::from)
            .filter(|path| path.exists())
        {
            return path;
        }
        [self.cwd.join("lock.toml"), self.config_file()]
            .into_iter()
            .find(|path| path.exists())
            .unwrap_or_else(|| self.config_file())
    }

    pub fn config_dir(&self) -> PathBuf {
        self.xdg_config_home
            .clone()
            .or_else(|| self.home.clone().map(|home| home.join(".config")))
            .unwrap_or_default()
            .join("glimpse")
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir().join("lock.toml")
    }
}

pub enum LockConfigEvent {
    Changed(LockConfig),
}

pub async fn watch_for_lock_config_changes(sender: mpsc::Sender<LockConfigEvent>) {
    watch_lock_config_file(LockConfig::detect_config_file(), sender).await;
}

async fn watch_lock_config_file(config_file: PathBuf, sender: mpsc::Sender<LockConfigEvent>) {
    let watch_file = config_file
        .canonicalize()
        .unwrap_or_else(|_| config_file.clone());
    let Some(config_dir) = config_file.parent().map(PathBuf::from) else {
        tracing::error!("lock config file has no parent directory");
        return;
    };

    if let Err(err) = fs::create_dir_all(&config_dir) {
        tracing::error!("failed to create lock config directory: {err}");
        return;
    }

    tracing::info!(
        config_file = %watch_file.display(),
        "watching lock config file for changes"
    );

    let handler_file = watch_file.clone();
    let handler_sender = sender.clone();
    let mut debouncer = match new_debouncer(
        Duration::from_millis(200),
        None,
        move |res: DebounceEventResult| {
            let events = match res {
                Ok(events) => events,
                Err(_) => return,
            };

            for event in events {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        if event
                            .paths
                            .iter()
                            .any(|path| path_matches(path, &handler_file))
                        {
                            let event =
                                LockConfigEvent::Changed(LockConfig::load_from_file(&handler_file));
                            if let Err(err) = handler_sender.try_send(event) {
                                tracing::error!(
                                    "failed to broadcast lock config change to the app: {err}"
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        },
    ) {
        Ok(debouncer) => debouncer,
        Err(err) => {
            tracing::error!("failed to create lock config watcher: {err}");
            return;
        }
    };

    if let Err(err) = debouncer.watch(&config_dir, notify::RecursiveMode::Recursive) {
        tracing::error!("failed to watch lock config directory: {err}");
        return;
    }

    sender.closed().await;
}

fn path_matches(path: &Path, expected: &Path) -> bool {
    path == expected
        || path
            .canonicalize()
            .map(|path| path == expected)
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use glimpse_core::{FitMode, WallpaperConfig};

    use super::{LockConfig, LockConfigDiscovery, LockControlButton, resolve_lock_spec};

    #[test]
    fn default_lock_config_uses_wallpaper_background_and_safe_auth_defaults() {
        let config = LockConfig::default();

        assert_eq!(config.pam_service, "glimpse-lock");
        assert_eq!(config.css_path, "themes/lock.css");
        assert!(config.background.path.is_none());
        assert!(config.background.color.is_none());
        assert_eq!(config.background.fit, None);
        assert_eq!(config.background.blur_radius, 0);
        assert_eq!(config.background.dim, 0.35);
        assert_eq!(
            config.controls.buttons,
            vec![
                LockControlButton::Weather,
                LockControlButton::Wifi,
                LockControlButton::Input,
                LockControlButton::Battery,
                LockControlButton::Power,
            ]
        );
    }

    #[test]
    fn lock_config_discovery_prefers_env_then_cwd_then_xdg() {
        let unique = format!(
            "glimpse-lock-config-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let env_file = root.join("env/lock.toml");
        let cwd_file = root.join("cwd/lock.toml");
        let xdg_file = root.join("xdg/glimpse/lock.toml");
        std::fs::create_dir_all(env_file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(cwd_file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(xdg_file.parent().unwrap()).unwrap();
        std::fs::write(&env_file, "").unwrap();
        std::fs::write(&cwd_file, "").unwrap();
        std::fs::write(&xdg_file, "").unwrap();

        let discovery = LockConfigDiscovery::new(
            HashMap::from([("GLIMPSE_LOCK_CONFIG".into(), env_file.display().to_string())]),
            root.join("cwd"),
            Some(root.join("xdg")),
            None,
        );
        assert_eq!(discovery.detect_config_file(), env_file);

        let discovery = LockConfigDiscovery::new(
            HashMap::new(),
            root.join("cwd"),
            Some(root.join("xdg")),
            None,
        );
        assert_eq!(discovery.detect_config_file(), cwd_file);

        let discovery = LockConfigDiscovery::new(
            HashMap::new(),
            root.join("other"),
            Some(root.join("xdg")),
            None,
        );
        assert_eq!(discovery.detect_config_file(), xdg_file);
    }

    #[test]
    fn lock_toml_parses_without_lock_table() {
        let config = LockConfig::from_toml_str(
            r##"
pam_service = "login"
css_path = "themes/custom-lock.css"

[background]
color = "#112233"
path = "/tmp/lock.png"
fit = "contain"
blur_radius = 8
dim = 0.5

[clock]
enabled = true
time_format = "%H:%M"
date_format = "%A, %B %-d"

[controls]
buttons = ["wifi", "input", "power"]
"##,
        )
        .expect("lock config should parse");

        assert_eq!(config.pam_service, "login");
        assert_eq!(config.css_path, "themes/custom-lock.css");
        assert_eq!(config.background.color.as_deref(), Some("#112233"));
        assert_eq!(config.background.path, Some(PathBuf::from("/tmp/lock.png")));
        assert_eq!(config.background.fit, Some(FitMode::Contain));
        assert_eq!(config.background.blur_radius, 8);
        assert_eq!(config.background.dim, 0.5);
        assert!(config.clock.enabled);
        assert_eq!(config.clock.time_format, "%H:%M");
        assert_eq!(
            config.controls.buttons,
            vec![
                LockControlButton::Wifi,
                LockControlButton::Input,
                LockControlButton::Power
            ]
        );
    }

    #[test]
    fn lock_background_resolves_from_wallpaper_when_unset() {
        let lock = LockConfig {
            pam_service: "login".into(),
            ..LockConfig::default()
        };
        let wallpaper = WallpaperConfig {
            color: "#112233".into(),
            path: Some(PathBuf::from("/tmp/wallpaper.png")),
            fit: FitMode::Contain,
            ..WallpaperConfig::default()
        };

        let spec = resolve_lock_spec(&lock, &wallpaper, &PathBuf::from("/tmp/config"));

        assert_eq!(spec.pam_service, "login");
        assert_eq!(spec.background.color, "#112233");
        assert_eq!(
            spec.background
                .image
                .as_ref()
                .map(|image| image.path.as_path()),
            Some(std::path::Path::new("/tmp/wallpaper.png"))
        );
        assert_eq!(spec.background.image.unwrap().fit, FitMode::Contain);
        assert_eq!(spec.css_path, PathBuf::from("/tmp/config/themes/lock.css"));
    }

    #[test]
    fn lock_background_overrides_wallpaper_when_set() {
        let lock = LockConfig::from_toml_str(
            r##"
[background]
color = "#445566"
path = "/tmp/lock.png"
fit = "fill"
blur_radius = 12
dim = 0.5
"##,
        )
        .expect("lock config should parse");
        let wallpaper = WallpaperConfig {
            color: "#112233".into(),
            path: Some(PathBuf::from("/tmp/wallpaper.png")),
            ..WallpaperConfig::default()
        };

        let spec = resolve_lock_spec(&lock, &wallpaper, &PathBuf::from("/tmp/config"));

        assert_eq!(spec.background.color, "#445566");
        assert_eq!(
            spec.background
                .image
                .as_ref()
                .map(|image| image.path.as_path()),
            Some(std::path::Path::new("/tmp/lock.png"))
        );
        assert_eq!(spec.background.image.unwrap().fit, FitMode::Fill);
        assert_eq!(spec.background.blur_radius, 12);
        assert_eq!(spec.background.dim, 0.5);
    }

    #[test]
    fn lock_background_fit_can_override_wallpaper_image_fit() {
        let lock = LockConfig::from_toml_str(
            r##"
[background]
fit = "contain"
"##,
        )
        .expect("lock config should parse");
        let wallpaper = WallpaperConfig {
            color: "#112233".into(),
            path: Some(PathBuf::from("/tmp/wallpaper.png")),
            fit: FitMode::Cover,
            ..WallpaperConfig::default()
        };

        let spec = resolve_lock_spec(&lock, &wallpaper, &PathBuf::from("/tmp/config"));

        assert_eq!(
            spec.background
                .image
                .as_ref()
                .map(|image| image.path.as_path()),
            Some(std::path::Path::new("/tmp/wallpaper.png"))
        );
        assert_eq!(spec.background.image.unwrap().fit, FitMode::Contain);
    }

    #[test]
    fn lock_controls_resolve_from_config() {
        let lock = LockConfig::from_toml_str(
            r#"
[controls]
buttons = ["battery", "power"]
"#,
        )
        .expect("lock config should parse");

        let spec = resolve_lock_spec(
            &lock,
            &WallpaperConfig::default(),
            &PathBuf::from("/tmp/config"),
        );

        assert_eq!(
            spec.controls,
            vec![LockControlButton::Battery, LockControlButton::Power]
        );
    }
}
