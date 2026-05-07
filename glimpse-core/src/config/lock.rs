use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{FitMode, ResolvedImageSpec, WallpaperConfig};

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
                LockControlButton::Wifi,
                LockControlButton::Input,
                LockControlButton::Weather,
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

#[cfg(test)]
mod tests {
    use super::*;

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
                LockControlButton::Wifi,
                LockControlButton::Input,
                LockControlButton::Weather,
                LockControlButton::Battery,
                LockControlButton::Power,
            ]
        );
    }

    #[test]
    fn config_parses_lock_block() {
        let config = crate::Config::from_toml_str(
            r##"
[lock]
pam_service = "login"
css_path = "themes/custom-lock.css"

[lock.background]
color = "#112233"
path = "/tmp/lock.png"
fit = "contain"
blur_radius = 8
dim = 0.5

[lock.clock]
enabled = true
time_format = "%H:%M"
date_format = "%A, %B %-d"

[lock.controls]
buttons = ["wifi", "input", "power"]
"##,
        )
        .expect("config should parse");

        assert_eq!(config.lock.pam_service, "login");
        assert_eq!(config.lock.css_path, "themes/custom-lock.css");
        assert_eq!(config.lock.background.color.as_deref(), Some("#112233"));
        assert_eq!(
            config.lock.background.path,
            Some(PathBuf::from("/tmp/lock.png"))
        );
        assert_eq!(config.lock.background.fit, Some(FitMode::Contain));
        assert_eq!(config.lock.background.blur_radius, 8);
        assert_eq!(config.lock.background.dim, 0.5);
        assert!(config.lock.clock.enabled);
        assert_eq!(config.lock.clock.time_format, "%H:%M");
        assert_eq!(
            config.lock.controls.buttons,
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
        let config = crate::Config::from_toml_str(
            r##"
[lock.background]
color = "#445566"
path = "/tmp/lock.png"
fit = "fill"
blur_radius = 12
dim = 0.5
"##,
        )
        .expect("config should parse");
        let wallpaper = WallpaperConfig {
            color: "#112233".into(),
            path: Some(PathBuf::from("/tmp/wallpaper.png")),
            ..WallpaperConfig::default()
        };

        let spec = resolve_lock_spec(&config.lock, &wallpaper, &PathBuf::from("/tmp/config"));

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
}
