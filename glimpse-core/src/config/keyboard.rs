use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{AppletConfig, AppletType, Config};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum KeyboardRememberMode {
    #[default]
    Global,
    App,
    Window,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct KeyboardConfig {
    pub remember: KeyboardRememberMode,
    pub labels: HashMap<String, String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct LegacyKeyboardConfig {
    labels: HashMap<String, String>,
}

impl KeyboardConfig {
    pub fn from_config(config: &Config) -> Self {
        let mut keyboard = config.keyboard.clone();
        if keyboard.labels.is_empty() {
            keyboard.labels = legacy_applet_labels(config);
        }
        keyboard
    }
}

fn legacy_applet_labels(config: &Config) -> HashMap<String, String> {
    config
        .applets
        .get("keyboard")
        .filter(|applet| is_keyboard_applet("keyboard", applet))
        .and_then(|applet| applet_label_config(Some(applet)))
        .or_else(|| {
            let mut applets = config
                .applets
                .iter()
                .filter(|(name, applet)| {
                    name.as_str() != "keyboard" && is_keyboard_applet(name, applet)
                })
                .collect::<Vec<_>>();
            applets.sort_by_key(|(name, _)| name.as_str());
            applets
                .into_iter()
                .find_map(|(_, applet)| applet_label_config(Some(applet)))
        })
        .unwrap_or_default()
}

fn is_keyboard_applet(name: &str, applet: &AppletConfig) -> bool {
    applet.extends == Some(AppletType::Keyboard) || (applet.extends.is_none() && name == "keyboard")
}

fn applet_label_config(config: Option<&AppletConfig>) -> Option<HashMap<String, String>> {
    config?
        .settings
        .clone()
        .try_into::<LegacyKeyboardConfig>()
        .map(|config| config.labels)
        .ok()
        .filter(|labels| !labels.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppletConfig;

    #[test]
    fn keyboard_config_defaults_to_global_without_labels() {
        assert_eq!(
            KeyboardConfig::default().remember,
            KeyboardRememberMode::Global
        );
        assert!(KeyboardConfig::default().labels.is_empty());
    }

    #[test]
    fn keyboard_config_falls_back_to_legacy_applet_labels() {
        let mut config = Config::default();
        config.applets.insert(
            "keyboard".into(),
            AppletConfig {
                settings: toml::toml! {
                    [labels]
                    us = "EN"
                }
                .into(),
                ..AppletConfig::default()
            },
        );

        let keyboard = KeyboardConfig::from_config(&config);

        assert_eq!(keyboard.labels.get("us"), Some(&"EN".into()));
    }

    #[test]
    fn keyboard_config_falls_back_to_custom_legacy_keyboard_applet_labels() {
        let mut config = Config::default();
        config.applets.insert(
            "my_keyboard".into(),
            AppletConfig {
                extends: Some(AppletType::Keyboard),
                settings: toml::toml! {
                    [labels]
                    us = "EN"
                }
                .into(),
            },
        );

        let keyboard = KeyboardConfig::from_config(&config);

        assert_eq!(keyboard.labels.get("us"), Some(&"EN".into()));
    }

    #[test]
    fn top_level_keyboard_labels_win_over_legacy_applet_labels() {
        let mut config = Config {
            keyboard: KeyboardConfig {
                labels: HashMap::from([("us".into(), "US".into())]),
                ..KeyboardConfig::default()
            },
            ..Config::default()
        };
        config.applets.insert(
            "keyboard".into(),
            AppletConfig {
                settings: toml::toml! {
                    [labels]
                    us = "EN"
                }
                .into(),
                ..AppletConfig::default()
            },
        );

        let keyboard = KeyboardConfig::from_config(&config);

        assert_eq!(keyboard.labels.get("us"), Some(&"US".into()));
    }
}
