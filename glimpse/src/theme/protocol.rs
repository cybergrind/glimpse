use serde::Serialize;

use crate::config::ThemeConfig;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreference {
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ThemePreferenceSnapshot {
    pub effective_mode: ThemePreference,
    pub backend: String,
    pub writable: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub enum ThemeHealth {
    #[default]
    Starting,
    Ready,
    Degraded {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeSource {
    #[default]
    Manual,
    SolarAuto,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ThemeState {
    pub health: ThemeHealth,
    pub config: ThemeConfig,
    pub effective_mode: ThemePreference,
    pub source: ThemeSource,
    pub last_applied_mode: Option<ThemePreference>,
}

impl Default for ThemeState {
    fn default() -> Self {
        Self {
            health: ThemeHealth::Starting,
            config: ThemeConfig::default(),
            effective_mode: ThemePreference::Light,
            source: ThemeSource::Manual,
            last_applied_mode: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeCommand {
    Refresh,
    ApplyConfig(ThemeConfig),
}

#[cfg(test)]
mod tests {
    use super::{ThemeCommand, ThemeHealth, ThemePreference, ThemeSource, ThemeState};
    use crate::config::{ThemeConfig, ThemeMode};

    #[test]
    fn theme_state_defaults_to_starting_light_mode() {
        let state = ThemeState::default();
        assert_eq!(state.health, ThemeHealth::Starting);
        assert_eq!(state.effective_mode, ThemePreference::Light);
        assert_eq!(state.source, ThemeSource::Manual);
        assert_eq!(state.last_applied_mode, None);
    }

    #[test]
    fn apply_config_command_carries_theme_config() {
        let config = ThemeConfig {
            name: Some("adwaita".into()),
            mode: ThemeMode::Auto,
        };
        assert_eq!(
            ThemeCommand::ApplyConfig(config.clone()),
            ThemeCommand::ApplyConfig(config)
        );
    }
}
