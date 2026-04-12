use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use css_color::Srgb;
use serde::Deserialize;

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

#[cfg(test)]
mod tests {
    use super::{BackdropConfig, BackdropMode};
    use std::path::PathBuf;

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
}
