use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

use gtk4::ContentFit;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{night_light::NightLightConfig, services::location::service::LocationSourceType};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PanelPosition {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
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

fn default_panel_height() -> i32 {
    36
}

fn default_clock_format() -> String {
    "%H:%M".to_string()
}

fn default_timezone_format() -> String {
    default_clock_format()
}

fn default_exec_restart_delay_ms() -> u64 {
    10_000
}

fn default_exec_options() -> Value {
    Value::Object(Default::default())
}

fn default_keyboard_format() -> KeyboardFormat {
    KeyboardFormat::Short
}

fn default_keyboard_per_window() -> bool {
    true
}

fn default_pager_style() -> PagerStyle {
    PagerStyle::Pills
}

fn default_pager_count() -> u32 {
    10
}

fn default_tray_icon_size() -> i32 {
    16
}

fn default_wallpaper_transition_ms() -> u32 {
    800
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    Auto,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PanelThemeMode {
    Light,
    #[default]
    Dark,
    Auto,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: Option<String>,
    pub mode: ThemeMode,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PanelConfig {
    pub position: PanelPosition,
    #[serde(default = "default_panel_height")]
    pub height: i32,
    #[serde(default)]
    pub theme_mode: PanelThemeMode,
    #[serde(default)]
    pub margin: Margin,
    #[serde(default)]
    pub left: Vec<String>,
    #[serde(default)]
    pub center: Vec<String>,
    #[serde(default)]
    pub right: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AppletConfig {
    pub extends: String,
    #[serde(flatten)]
    pub settings: toml::Value,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AudioConfig {
    pub show_icon: bool,
    pub show_mic_indicator: bool,
    pub label_format: String,
    pub tooltip_format: String,
    pub scroll_step: u32,
    pub max_volume: u32,
    pub settings_command: String,
    pub show_streams: bool,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            show_icon: true,
            show_mic_indicator: true,
            label_format: String::new(),
            tooltip_format: "{device} — {volume}%".into(),
            scroll_step: 10,
            max_volume: 100,
            settings_command: "pavucontrol".into(),
            show_streams: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BatteryConfig {
    pub show_icon: bool,
    pub label_on_battery: String,
    pub label_on_ac: String,
    pub tooltip_on_battery: String,
    pub tooltip_on_ac: String,
    pub settings_command: String,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            show_icon: true,
            label_on_battery: "{percentage}%".into(),
            label_on_ac: String::new(),
            tooltip_on_battery: "{percentage}% {state}, {time_left}".into(),
            tooltip_on_ac: "{percentage}% {state}".into(),
            settings_command: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BluetoothConfig {
    pub settings_command: String,
}

impl Default for BluetoothConfig {
    fn default() -> Self {
        Self {
            settings_command: "blueman-manager".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BrightnessConfig {
    pub show_icon: bool,
    pub label_format: String,
    pub scroll_step: u32,
    pub hide_when_unavailable: bool,
    pub settings_command: String,
}

impl Default for BrightnessConfig {
    fn default() -> Self {
        Self {
            show_icon: true,
            label_format: String::new(),
            scroll_step: 5,
            hide_when_unavailable: true,
            settings_command: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TimezoneEntry {
    pub name: String,
    pub timezone: String,
    #[serde(default = "default_timezone_format")]
    pub format: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ClockConfig {
    #[serde(default = "default_clock_format")]
    pub format: String,
    #[serde(default)]
    pub timezones: Vec<TimezoneEntry>,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            format: default_clock_format(),
            timezones: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ExecConfig {
    pub command: Vec<String>,
    pub restart_delay_ms: u64,
    #[serde(default = "default_exec_options")]
    pub options: Value,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            command: Vec::new(),
            restart_delay_ms: default_exec_restart_delay_ms(),
            options: default_exec_options(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardFormat {
    Short,
    Full,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct KeyboardConfig {
    #[serde(default = "default_keyboard_format")]
    pub format: KeyboardFormat,
    #[serde(default = "default_keyboard_per_window")]
    pub per_window: bool,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            format: default_keyboard_format(),
            per_window: default_keyboard_per_window(),
            labels: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MprisConfig {
    pub label_format: String,
    pub show_artwork: bool,
    pub hide_when_empty: bool,
    pub max_rows: usize,
}

impl Default for MprisConfig {
    fn default() -> Self {
        Self {
            label_format: "{artist} - {track}".into(),
            show_artwork: true,
            hide_when_empty: true,
            max_rows: 6,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct NetworkConfig {
    pub label_format: String,
    pub tooltip_format: String,
    pub show_vpn_icon: bool,
    pub settings_command: String,
    pub scan_interval: u64,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            label_format: String::new(),
            tooltip_format: String::new(),
            show_vpn_icon: true,
            settings_command: "nm-connection-editor".into(),
            scan_interval: 15,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct NotificationsConfig {
    pub popup_position: String,
    pub popup_margin_top: i32,
    pub popup_timeout: u32,
    pub history_limit: u32,
    pub show_popup: bool,
    pub badge_style: String,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            popup_position: "top-center".into(),
            popup_margin_top: 12,
            popup_timeout: 5000,
            history_limit: 100,
            show_popup: true,
            badge_style: "dot".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PagerStyle {
    Pills,
    Numbered,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScrollAction {
    Windows,
    Workspaces,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PagerConfig {
    #[serde(default = "default_pager_style")]
    pub style: PagerStyle,
    #[serde(default = "default_pager_count")]
    pub count: u32,
    #[serde(default)]
    pub scroll_action: Option<ScrollAction>,
}

impl Default for PagerConfig {
    fn default() -> Self {
        Self {
            style: default_pager_style(),
            count: default_pager_count(),
            scroll_action: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PowerConfig {
    pub percentage: bool,
    pub low_battery_treshold: u8,
    pub hide_on_no_battery: bool,
    pub format: String,
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            percentage: false,
            low_battery_treshold: 15,
            hide_on_no_battery: true,
            format: String::from("{}%"),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PrivacyConfig {}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SessionConfig {
    pub show_lock: bool,
    pub show_logout: bool,
    pub show_suspend: bool,
    pub show_hibernate: bool,
    pub show_reboot: bool,
    pub show_shutdown: bool,
    pub confirm_logout: bool,
    pub confirm_suspend: bool,
    pub confirm_hibernate: bool,
    pub confirm_reboot: bool,
    pub confirm_shutdown: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            show_lock: true,
            show_logout: true,
            show_suspend: true,
            show_hibernate: false,
            show_reboot: true,
            show_shutdown: true,
            confirm_logout: true,
            confirm_suspend: true,
            confirm_hibernate: true,
            confirm_reboot: true,
            confirm_shutdown: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TrayConfig {
    #[serde(default = "default_tray_icon_size")]
    pub icon_size: i32,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            icon_size: default_tray_icon_size(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WeatherConfig {
    pub city_name: String,
    pub geolocate: bool,
    pub hourly_slots: usize,
    pub forecast_days: usize,
    pub label_format: String,
    pub tooltip_format: String,
    pub refresh_interval: u64,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            city_name: String::new(),
            geolocate: false,
            hourly_slots: 5,
            forecast_days: 5,
            label_format: "{temp}°".into(),
            tooltip_format: "{condition} · {temp}° · {location}".into(),
            refresh_interval: 1800,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WallpaperMode {
    #[default]
    Color,
    Image,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ImageFit {
    Fill,
    Contain,
    #[default]
    Cover,
}

impl ImageFit {
    pub fn to_gtk(&self) -> ContentFit {
        match self {
            ImageFit::Fill => ContentFit::Fill,
            ImageFit::Contain => ContentFit::Contain,
            ImageFit::Cover => ContentFit::Cover,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WallpaperConfig {
    pub color: String,
    #[serde(default = "default_wallpaper_transition_ms")]
    pub transition_ms: u32,
    pub mode: WallpaperMode,
    pub path: Option<PathBuf>,
    pub fit: ImageFit,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            color: "transparent".to_owned(),
            transition_ms: default_wallpaper_transition_ms(),
            mode: WallpaperMode::default(),
            path: None,
            fit: ImageFit::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BackdropConfig {
    pub enabled: bool,
    pub path: Option<PathBuf>,
    pub blur_radius: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LocationConfig {
    pub enabled: bool,
    pub source: LocationSourceType,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    path: Option<PathBuf>,
    #[serde(default)]
    pub panels: Vec<PanelConfig>,
    #[serde(default)]
    pub applets: HashMap<String, AppletConfig>,
    #[serde(default)]
    pub wallpaper: WallpaperConfig,
    #[serde(default)]
    pub backdrop: BackdropConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub location: LocationConfig,
    #[serde(default)]
    pub night_light: NightLightConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: None,
            applets: HashMap::new(),
            wallpaper: WallpaperConfig::default(),
            backdrop: BackdropConfig::default(),
            theme: ThemeConfig::default(),
            night_light: NightLightConfig::default(),
            location: LocationConfig {
                enabled: false,
                source: LocationSourceType::GeoClue,
                latitude: None,
                longitude: None,
            },
            panels: vec![PanelConfig {
                height: default_panel_height(),
                theme_mode: PanelThemeMode::default(),
                margin: Margin::default(),
                position: PanelPosition::Bottom,
                left: vec![],
                center: vec![],
                right: vec![],
            }],
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
                Err(error) => {
                    tracing::error!("failed to parse {}: {}", config_path.display(), error);
                    return Self {
                        path: Some(config_path),
                        ..Default::default()
                    };
                }
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

    pub fn themes_directory() -> PathBuf {
        Self::config_directory().join("themes")
    }

    pub fn theme_path_for_name(name: &str) -> PathBuf {
        Self::themes_directory().join(format!("{name}.css"))
    }

    pub fn default_theme_name() -> &'static str {
        "adwaita"
    }

    pub fn default_theme_path() -> PathBuf {
        Self::theme_path_for_name(Self::default_theme_name())
    }

    pub fn active_theme_path(&self) -> Option<PathBuf> {
        self.theme
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(Self::theme_path_for_name)
    }

    pub fn theme_path(&self) -> PathBuf {
        self.active_theme_path()
            .unwrap_or_else(Self::default_theme_path)
    }

    fn config_path() -> Option<PathBuf> {
        let config_path = Self::config_directory().join("config.toml");
        tracing::debug!("possible config file {:?}", config_path);
        config_path.exists().then_some(config_path)
    }

    pub fn persist_background_settings(
        &self,
        wallpaper: &WallpaperConfig,
        backdrop: &BackdropConfig,
    ) -> Result<PathBuf, String> {
        let path = self
            .path
            .clone()
            .unwrap_or_else(|| Self::config_directory().join("config.toml"));
        persist_background_settings_at_path(&path, wallpaper, backdrop)?;
        Ok(path)
    }
}

pub fn persist_background_settings_at_path(
    path: &Path,
    wallpaper: &WallpaperConfig,
    backdrop: &BackdropConfig,
) -> Result<(), String> {
    let mut document = if path.exists() {
        let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
        toml::from_str::<toml::Value>(&content).map_err(|error| error.to_string())?
    } else {
        toml::Value::Table(Default::default())
    };

    let table = document
        .as_table_mut()
        .ok_or("glimpse config root must be a TOML table")?;
    table.insert("wallpaper".into(), wallpaper_toml_value(wallpaper));
    table.insert("backdrop".into(), backdrop_toml_value(backdrop));

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let content = toml::to_string_pretty(&document).map_err(|error| error.to_string())?;
    fs::write(path, content).map_err(|error| error.to_string())
}

fn wallpaper_toml_value(config: &WallpaperConfig) -> toml::Value {
    let mut table = toml::map::Map::new();
    table.insert("color".into(), toml::Value::String(config.color.clone()));
    table.insert(
        "transition_ms".into(),
        toml::Value::Integer(i64::from(config.transition_ms)),
    );
    table.insert(
        "mode".into(),
        toml::Value::String(match config.mode {
            WallpaperMode::Color => "color".into(),
            WallpaperMode::Image => "image".into(),
        }),
    );
    if let Some(path) = &config.path {
        table.insert(
            "path".into(),
            toml::Value::String(path.to_string_lossy().into_owned()),
        );
    }
    table.insert(
        "fit".into(),
        toml::Value::String(match config.fit {
            ImageFit::Fill => "fill".into(),
            ImageFit::Contain => "contain".into(),
            ImageFit::Cover => "cover".into(),
        }),
    );
    toml::Value::Table(table)
}

fn backdrop_toml_value(config: &BackdropConfig) -> toml::Value {
    let mut table = toml::map::Map::new();
    table.insert("enabled".into(), toml::Value::Boolean(config.enabled));
    if let Some(path) = &config.path {
        table.insert(
            "path".into(),
            toml::Value::String(path.to_string_lossy().into_owned()),
        );
    }
    table.insert(
        "blur_radius".into(),
        toml::Value::Integer(i64::from(config.blur_radius)),
    );
    toml::Value::Table(table)
}

#[cfg(test)]
mod tests {
    use super::{
        BackdropConfig, BrightnessConfig, Config, ExecConfig, ImageFit, PanelConfig, PanelPosition,
        PanelThemeMode, ThemeMode, WallpaperConfig, WallpaperMode, WeatherConfig,
    };
    use crate::night_light::NightLightSchedule;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should work")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("glimpse-config-{label}-{unique}"));
        fs::create_dir_all(&path).expect("temp dir should exist");
        path
    }

    #[test]
    fn default_brightness_config_shows_icon_and_internal_label() {
        let config = BrightnessConfig::default();
        assert!(config.show_icon);
        assert_eq!(config.label_format, "");
        assert_eq!(config.scroll_step, 5);
    }

    #[test]
    fn exec_config_defaults_restart_delay() {
        let config: ExecConfig =
            toml::from_str("command = [\"echo\", \"hello\"]").expect("config should parse");

        assert_eq!(
            config.command,
            vec!["echo".to_string(), "hello".to_string()]
        );
        assert_eq!(config.restart_delay_ms, 10_000);
    }

    #[test]
    fn exec_config_accepts_explicit_restart_delay() {
        let config: ExecConfig =
            toml::from_str("command = [\"custom-applet\"]\nrestart_delay_ms = 2500")
                .expect("config should parse");

        assert_eq!(config.command, vec!["custom-applet".to_string()]);
        assert_eq!(config.restart_delay_ms, 2_500);
    }

    #[test]
    fn theme_config_parses_auto_mode() {
        let config: Config = toml::from_str(
            r#"
[theme]
mode = "auto"
"#,
        )
        .expect("config should parse");

        assert_eq!(config.theme.mode, ThemeMode::Auto);
        assert_eq!(config.theme.name, None);
    }

    #[test]
    fn default_weather_config_uses_city_and_disables_ip_fallback() {
        let cfg = WeatherConfig::default();
        assert_eq!(cfg.city_name, "");
        assert!(!cfg.geolocate);
    }

    #[test]
    fn default_weather_config_uses_five_forecast_days() {
        let cfg = WeatherConfig::default();
        assert_eq!(cfg.forecast_days, 5);
    }

    #[test]
    fn default_weather_config_uses_five_hourly_slots() {
        let cfg = WeatherConfig::default();
        assert_eq!(cfg.hourly_slots, 5);
    }

    #[test]
    fn default_night_light_config_is_off() {
        let cfg = Config::default().night_light;
        assert_eq!(cfg.temperature, 4200);
        assert_eq!(cfg.schedule, NightLightSchedule::Off);
        assert_eq!(cfg.latitude, None);
        assert_eq!(cfg.longitude, None);
        assert_eq!(cfg.start_time, None);
        assert_eq!(cfg.end_time, None);
        assert_eq!(cfg.transition_minutes, 15);
    }

    #[test]
    fn config_parses_night_light_block() {
        let config: Config = toml::from_str(
            r#"
[night_light]
temperature = 4200
schedule = "schedule"
latitude = 52.2298
longitude = 21.0118
start_time = "18:00"
end_time = "06:30"
transition_minutes = 75
"#,
        )
        .expect("config should parse");

        assert_eq!(config.night_light.temperature, 4200);
        assert_eq!(config.night_light.schedule, NightLightSchedule::Schedule);
        assert_eq!(config.night_light.latitude, Some(52.2298));
        assert_eq!(config.night_light.longitude, Some(21.0118));
        assert_eq!(config.night_light.start_time.as_deref(), Some("18:00"));
        assert_eq!(config.night_light.end_time.as_deref(), Some("06:30"));
        assert_eq!(config.night_light.transition_minutes, 75);
    }

    #[test]
    fn panel_sections_default_to_empty_lists() {
        let panel: PanelConfig = toml::from_str(
            r#"
position = "top"
"#,
        )
        .expect("panel config");

        assert!(panel.left.is_empty());
        assert!(panel.center.is_empty());
        assert!(panel.right.is_empty());
        assert_eq!(panel.theme_mode, PanelThemeMode::Dark);
    }

    #[test]
    fn panel_config_parses_explicit_sections() {
        let panel: PanelConfig = toml::from_str(
            r#"
position = "top"
left = ["pager", "clock"]
center = ["mpris"]
right = ["network", "network", "tray"]
"#,
        )
        .expect("panel config");

        assert_eq!(panel.position, PanelPosition::Top);
        assert_eq!(panel.left, vec!["pager", "clock"]);
        assert_eq!(panel.center, vec!["mpris"]);
        assert_eq!(panel.right, vec!["network", "network", "tray"]);
        assert_eq!(panel.theme_mode, PanelThemeMode::Dark);
    }

    #[test]
    fn panel_config_parses_explicit_theme_mode() {
        let panel: PanelConfig = toml::from_str(
            r#"
position = "top"
theme_mode = "light"
"#,
        )
        .expect("panel config");

        assert_eq!(panel.theme_mode, PanelThemeMode::Light);
    }

    #[test]
    fn config_loads_named_exec_applet_blocks() {
        let config: Config = toml::from_str(
            r#"
[[panels]]
position = "top"
left = ["sysinfo"]

[applets.sysinfo]
extends = "exec"
command = ["/tmp/sysstats-applet"]
"#,
        )
        .expect("config should parse");

        let sysinfo = config
            .applets
            .get("sysinfo")
            .expect("sysinfo applet config should be present");
        assert_eq!(sysinfo.extends, "exec");

        let exec: ExecConfig = sysinfo
            .settings
            .clone()
            .try_into()
            .expect("sysinfo settings should parse as exec config");
        assert_eq!(exec.command, vec!["/tmp/sysstats-applet".to_string()]);
    }

    #[test]
    fn persist_background_settings_preserves_unrelated_config() {
        let root = temp_dir("persist-background-preserves");
        let config_path = root.join("config.toml");
        fs::write(
            &config_path,
            r#"
[[panels]]
position = "top"
left = ["clock"]

[applets.network]
extends = "network"
"#,
        )
        .expect("seed config should be written");

        let wallpaper = WallpaperConfig {
            color: "#203040".into(),
            transition_ms: 800,
            mode: WallpaperMode::Image,
            path: Some(PathBuf::from("/tmp/example.png")),
            fit: ImageFit::Cover,
        };
        let backdrop = BackdropConfig {
            enabled: true,
            path: Some(PathBuf::from("/tmp/backdrop.png")),
            blur_radius: 24,
        };

        super::persist_background_settings_at_path(&config_path, &wallpaper, &backdrop)
            .expect("background settings should persist");

        let written = fs::read_to_string(&config_path).expect("config should be readable");
        assert!(written.contains("[[panels]]"));
        assert!(written.contains("[applets.network]"));
        assert!(written.contains("[wallpaper]"));
        assert!(written.contains("[backdrop]"));
    }

    #[test]
    fn persist_background_settings_creates_missing_config_file() {
        let root = temp_dir("persist-background-create");
        let config_path = root.join("config.toml");
        let wallpaper = WallpaperConfig {
            color: "#101010".into(),
            transition_ms: 500,
            mode: WallpaperMode::Color,
            path: None,
            fit: ImageFit::Contain,
        };
        let backdrop = BackdropConfig {
            enabled: false,
            path: None,
            blur_radius: 8,
        };

        super::persist_background_settings_at_path(&config_path, &wallpaper, &backdrop)
            .expect("missing config should be created");

        let written = fs::read_to_string(&config_path).expect("config should exist");
        assert!(written.contains("[wallpaper]"));
        assert!(written.contains("mode = \"color\""));
        assert!(written.contains("[backdrop]"));
    }
}
