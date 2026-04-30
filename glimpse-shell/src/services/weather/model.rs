#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub city_name: String,
    pub hourly_slots: usize,
    pub forecast_days: usize,
    pub refresh_interval: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            city_name: String::new(),
            hourly_slots: 5,
            forecast_days: 5,
            refresh_interval: 1800,
        }
    }
}

impl Config {
    pub fn hourly_slots(&self) -> usize {
        self.hourly_slots.clamp(1, 8)
    }

    pub fn forecast_days(&self) -> usize {
        self.forecast_days.clamp(1, 10)
    }

    pub fn refresh_interval(&self) -> u64 {
        self.refresh_interval.max(60)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Unknown,
    Loading,
    Ready(Snapshot),
    Unavailable(String),
}

impl Default for State {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Configure(Config),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub current: CurrentWeather,
    pub hourly: Vec<HourlyForecast>,
    pub forecast: Vec<DailyForecast>,
    pub location: Location,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CurrentWeather {
    pub temperature: f64,
    pub apparent_temperature: f64,
    pub humidity: u8,
    pub weather_code: u32,
    pub condition: String,
    pub icon: String,
    pub wind_speed: f64,
    pub wind_direction: u16,
    pub wind_direction_label: String,
    pub pressure: f64,
    pub uv_index: f64,
    pub precipitation: f64,
    pub is_day: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct HourlyForecast {
    pub time: String,
    pub temperature: f64,
    pub weather_code: u32,
    pub condition: String,
    pub icon: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DailyForecast {
    pub date: String,
    pub day_name: String,
    pub is_today: bool,
    pub weather_code: u32,
    pub condition: String,
    pub icon: String,
    pub temperature_max: f64,
    pub temperature_min: f64,
    pub precipitation_sum: f64,
    pub sunrise: String,
    pub sunset: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
    pub city: String,
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn config_clamps_requested_slots_and_intervals() {
        let config = Config {
            hourly_slots: 99,
            forecast_days: 99,
            refresh_interval: 1,
            ..Config::default()
        };

        assert_eq!(config.hourly_slots(), 8);
        assert_eq!(config.forecast_days(), 10);
        assert_eq!(config.refresh_interval(), 60);
    }
}
