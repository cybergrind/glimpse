use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NightLightConfig {
    pub temperature: u32,
    pub schedule: NightLightSchedule,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub transition_minutes: u32,
}

impl Default for NightLightConfig {
    fn default() -> Self {
        Self {
            temperature: DEFAULT_NIGHT_LIGHT_TEMPERATURE_KELVIN,
            schedule: NightLightSchedule::Off,
            start_time: None,
            end_time: None,
            transition_minutes: DEFAULT_TRANSITION_MINUTES,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NightLightConfig, NightLightSchedule};

    #[test]
    fn night_light_config_defaults_to_off() {
        let config = NightLightConfig::default();

        assert_eq!(config.temperature, 4200);
        assert_eq!(config.schedule, NightLightSchedule::Off);
        assert_eq!(config.start_time, None);
        assert_eq!(config.end_time, None);
        assert_eq!(config.transition_minutes, 15);
    }

    #[test]
    fn partial_night_light_config_uses_defaults() {
        let config: NightLightConfig = toml::from_str(
            r#"
schedule = "automatic"
temperature = 4300
"#,
        )
        .expect("partial night light config should parse");

        assert_eq!(config.schedule, NightLightSchedule::Automatic);
        assert_eq!(config.temperature, 4300);
        assert_eq!(config.transition_minutes, 15);
    }

    #[test]
    fn legacy_manual_alias_parses_as_schedule() {
        let config: NightLightConfig = toml::from_str(
            r#"
schedule = "manual"
"#,
        )
        .expect("manual alias should parse");

        assert_eq!(config.schedule, NightLightSchedule::Schedule);
    }
}
