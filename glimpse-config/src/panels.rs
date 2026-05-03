use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::ThemeMode;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Left,
    Top,
    Right,
    Bottom,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct PanelConfig {
    pub size: i32,
    pub monitor: Option<String>,
    pub position: Position,
    pub margin: Margin,
    pub theme_mode: ThemeMode,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            size: 36,
            monitor: None,
            position: Position::Top,
            margin: Margin::default(),
            theme_mode: ThemeMode::Dark,
            left: vec!["pager".into(), "mpris".into()],
            center: vec!["clock".into(), "weather".into(), "notifications".into()],
            right: vec![
                "tray".into(),
                "keyboard".into(),
                "bluetooth".into(),
                "network".into(),
                "audio".into(),
                "battery".into(),
                "session".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AppletType {
    Audio,
    Battery,
    Bluetooth,
    Clock,
    Keyboard,
    Mpris,
    Network,
    Notifications,
    Pager,
    Session,
    Tray,
    Weather,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct AppletConfig {
    pub extends: Option<AppletType>,
    #[serde(flatten)]
    pub settings: toml::Value,
}

impl Default for AppletConfig {
    fn default() -> Self {
        Self {
            extends: None,
            settings: toml::Value::Table(toml::map::Map::new()),
        }
    }
}

pub type AppletConfigs = HashMap<String, AppletConfig>;
