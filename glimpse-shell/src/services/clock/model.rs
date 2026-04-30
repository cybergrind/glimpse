use chrono::{DateTime, Local};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct TimezoneConfig {
    pub name: String,
    pub timezone: String,
    pub format: String,
}

impl Default for TimezoneConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            timezone: "UTC".into(),
            format: "%H:%M".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub timezones: Vec<TimezoneConfig>,
    pub tick_interval: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            timezones: Vec::new(),
            tick_interval: 1,
        }
    }
}

impl Config {
    pub fn tick_interval(&self) -> u64 {
        self.tick_interval.clamp(1, 60)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Configure(Config),
    #[allow(dead_code)]
    Refresh,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub now: DateTime<Local>,
    pub world: Vec<WorldClockTime>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            now: Local::now(),
            world: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldClockTime {
    pub name: String,
    pub timezone: String,
    pub time: String,
    pub offset: String,
    pub day_label: Option<&'static str>,
}
