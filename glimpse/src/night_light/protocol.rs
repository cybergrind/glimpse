use serde::{Deserialize, Serialize};

use crate::compositor::CompositorKind;

pub const DAYLIGHT_TEMPERATURE_KELVIN: u32 = 6500;
pub const DEFAULT_NIGHT_LIGHT_TEMPERATURE_KELVIN: u32 = 4200;
pub const DEFAULT_TRANSITION_MINUTES: u32 = 15;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NightLightSchedule {
    #[default]
    Off,
    Automatic,
    #[serde(alias = "manual")]
    Schedule,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NightLightPhase {
    #[default]
    Disabled,
    Day,
    TransitionToNight,
    Night,
    TransitionToDay,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub enum NightLightHealth {
    #[default]
    Starting,
    Ready,
    Unsupported,
    Reconnecting {
        attempt: u32,
    },
    Degraded {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NightLightConfig {
    pub temperature: u32,
    pub schedule: NightLightSchedule,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub transition_minutes: u32,
}

impl Default for NightLightConfig {
    fn default() -> Self {
        Self {
            temperature: DEFAULT_NIGHT_LIGHT_TEMPERATURE_KELVIN,
            schedule: NightLightSchedule::Off,
            latitude: None,
            longitude: None,
            start_time: None,
            end_time: None,
            transition_minutes: DEFAULT_TRANSITION_MINUTES,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NightLightState {
    pub compositor: CompositorKind,
    pub health: NightLightHealth,
    pub config: NightLightConfig,
    pub phase: NightLightPhase,
    pub current_temperature_kelvin: u32,
    pub target_temperature_kelvin: u32,
    pub effective_temperature_kelvin: u32,
}

impl Default for NightLightState {
    fn default() -> Self {
        Self {
            compositor: CompositorKind::Unknown,
            health: NightLightHealth::Starting,
            config: NightLightConfig::default(),
            phase: NightLightPhase::Disabled,
            current_temperature_kelvin: DAYLIGHT_TEMPERATURE_KELVIN,
            target_temperature_kelvin: DAYLIGHT_TEMPERATURE_KELVIN,
            effective_temperature_kelvin: DAYLIGHT_TEMPERATURE_KELVIN,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum NightLightCommand {
    Refresh,
    ApplyConfig(NightLightConfig),
}

#[cfg(test)]
mod tests {
    use super::{
        NightLightConfig, NightLightHealth, NightLightPhase, NightLightSchedule, NightLightState,
    };
    use crate::compositor::CompositorKind;

    #[test]
    fn state_defaults_to_disabled_day_temperature() {
        let state = NightLightState::default();
        assert_eq!(state.health, NightLightHealth::Starting);
        assert_eq!(state.phase, NightLightPhase::Disabled);
        assert_eq!(state.compositor, CompositorKind::Unknown);
        assert_eq!(state.current_temperature_kelvin, 6500);
        assert_eq!(state.target_temperature_kelvin, 6500);
        assert_eq!(state.effective_temperature_kelvin, 6500);
    }

    #[test]
    fn config_defaults_to_off_schedule() {
        let config = NightLightConfig::default();
        assert_eq!(config.temperature, 4200);
        assert_eq!(config.schedule, NightLightSchedule::Off);
        assert_eq!(config.latitude, None);
        assert_eq!(config.longitude, None);
        assert_eq!(config.start_time, None);
        assert_eq!(config.end_time, None);
        assert_eq!(config.transition_minutes, 15);
    }

    #[test]
    fn partial_config_uses_default_transition_minutes() {
        let config: NightLightConfig = toml::from_str(
            r#"
schedule = "automatic"
temperature = 4200
"#,
        )
        .expect("partial config should parse");

        assert_eq!(config.schedule, NightLightSchedule::Automatic);
        assert_eq!(config.transition_minutes, 15);
        assert_eq!(config.latitude, None);
        assert_eq!(config.longitude, None);
    }
}
