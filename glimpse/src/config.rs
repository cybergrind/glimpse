use std::{collections::HashMap, env, fs, path::PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use css_color::Srgb;
use gtk4::ContentFit;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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

#[derive(Debug, Clone, Deserialize)]
pub struct PanelConfig {
    pub position: PanelPosition,
    #[serde(default = "default_panel_height")]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
pub struct TimezoneEntry {
    pub name: String,
    pub timezone: String,
    #[serde(default = "default_timezone_format")]
    pub format: String,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardFormat {
    Short,
    Full,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PagerStyle {
    Pills,
    Numbered,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ScrollAction {
    Windows,
    Workspaces,
}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PrivacyConfig {}

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WallpaperMode {
    #[default]
    Color,
    Image,
}

#[derive(Debug, Clone, Deserialize, Default)]
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

#[derive(Debug, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BackdropMode {
    #[default]
    Color,
    Image,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct BackdropConfig {
    pub enabled: bool,
    pub mode: BackdropMode,
    pub color: String,
    pub path: Option<PathBuf>,
    pub blur_radius: u32,
}

impl Default for BackdropConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: BackdropMode::Color,
            color: "transparent".to_owned(),
            path: None,
            blur_radius: 0,
        }
    }
}

impl BackdropConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        match self.mode {
            BackdropMode::Color => {
                self.color
                    .parse::<Srgb>()
                    .map_err(|_| anyhow!("invalid backdrop color '{}'", self.color))?;
                Ok(())
            }
            BackdropMode::Image => {
                let path = self
                    .path
                    .as_ref()
                    .context("backdrop image mode requires 'path'")?;
                if !path.is_file() {
                    bail!("backdrop image '{}' does not exist", path.display());
                }
                Ok(())
            }
        }
    }
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            path: None,
            applets: HashMap::new(),
            panels: vec![PanelConfig {
                height: default_panel_height(),
                margin: Margin::default(),
                position: PanelPosition::Bottom,
                applets: vec![],
            }],
            wallpaper: WallpaperConfig::default(),
            backdrop: BackdropConfig::default(),
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
                    tracing::error!("failed to parse {}: {}", config_path.display(), error)
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

    pub fn theme_path(&self) -> PathBuf {
        Self::config_directory().join("theme.css")
    }

    fn config_path() -> Option<PathBuf> {
        let config_path = Self::config_directory().join("config.toml");
        tracing::debug!("possible config file {:?}", config_path);
        config_path.exists().then_some(config_path)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackdropConfig, BackdropMode, BrightnessConfig, Config, ExecConfig, WeatherConfig,
    };
    use std::path::PathBuf;

    #[test]
    fn default_config_disables_backdrop() {
        let config = Config::default();

        assert!(!config.backdrop.enabled);
        assert_eq!(config.backdrop.color, "transparent");
    }

    #[test]
    fn default_config_is_disabled_transparent_color() {
        let config = BackdropConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.mode, BackdropMode::Color);
        assert_eq!(config.color, "transparent");
        assert_eq!(config.path, None);
        assert_eq!(config.blur_radius, 0);
    }

    #[test]
    fn image_mode_requires_path() {
        let config = BackdropConfig {
            enabled: true,
            mode: BackdropMode::Image,
            path: None,
            ..BackdropConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn image_mode_rejects_missing_file() {
        let config = BackdropConfig {
            enabled: true,
            mode: BackdropMode::Image,
            path: Some(PathBuf::from("/tmp/definitely-not-a-real-backdrop-file.png")),
            ..BackdropConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn color_mode_ignores_blur_radius() {
        let config = BackdropConfig {
            enabled: true,
            mode: BackdropMode::Color,
            blur_radius: 32,
            color: "transparent".to_owned(),
            ..BackdropConfig::default()
        };

        assert!(config.validate().is_ok());
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
}
