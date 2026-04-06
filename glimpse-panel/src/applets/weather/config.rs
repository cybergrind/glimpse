use serde::Deserialize;

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

#[cfg(test)]
mod tests {
    use super::WeatherConfig;

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
