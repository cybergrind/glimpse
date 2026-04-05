use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WeatherConfig {
    pub latitude: f64,
    pub longitude: f64,
    pub city_name: String,
    pub label_format: String,
    pub tooltip_format: String,
    pub refresh_interval: u64,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            latitude: 0.0,
            longitude: 0.0,
            city_name: String::new(),
            label_format: "{temp}°".into(),
            tooltip_format: "{condition} · {temp}°".into(),
            refresh_interval: 1800,
        }
    }
}
