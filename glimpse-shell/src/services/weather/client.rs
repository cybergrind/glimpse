use chrono::{DateTime, Local, NaiveDate};
use serde_json::Value;

use super::model::{Config, CurrentWeather, DailyForecast, HourlyForecast, Location, Snapshot};

const GEOCODE_API_BASE: &str = "https://geocoding-api.open-meteo.com/v1/search";
const FORECAST_API_BASE: &str = "https://api.open-meteo.com/v1/forecast";
const REVERSE_GEOCODE_API_BASE: &str = "https://nominatim.openstreetmap.org/reverse";
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct WeatherClient {
    http: reqwest::Client,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ForecastLocation {
    City(String),
    Coordinates(Location),
}

impl WeatherClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .user_agent("glimpse-shell")
                .build()
                .expect("weather HTTP client should build"),
        }
    }

    pub async fn fetch_snapshot(
        &self,
        location: ForecastLocation,
        config: &Config,
    ) -> Result<Snapshot, WeatherError> {
        let location = match location {
            ForecastLocation::City(city) => self.geocode_city(&city).await?,
            ForecastLocation::Coordinates(location) => {
                match self.reverse_geocode_location(&location).await {
                    Ok(location) => location,
                    Err(error) => {
                        tracing::debug!(%error, "failed to reverse geocode weather location");
                        location
                    }
                }
            }
        };
        self.fetch_forecast(location, config).await
    }

    async fn geocode_city(&self, city: &str) -> Result<Location, WeatherError> {
        let response = self
            .http
            .get(GEOCODE_API_BASE)
            .query(&[
                ("name", city),
                ("count", "1"),
                ("language", "en"),
                ("format", "json"),
            ])
            .send()
            .await
            .map_err(|error| WeatherError::Geocoding(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(WeatherError::Geocoding(format!(
                "geocoding returned {status}"
            )));
        }

        parse_geocoding_location(&response.json::<Value>().await.map_err(|error| {
            WeatherError::Geocoding(format!("failed to parse geocoding response: {error}"))
        })?)
        .ok_or_else(|| WeatherError::Geocoding(format!("no result for {city}")))
    }

    async fn reverse_geocode_location(
        &self,
        location: &Location,
    ) -> Result<Location, WeatherError> {
        let response = self
            .http
            .get(REVERSE_GEOCODE_API_BASE)
            .query(&[
                ("format", "jsonv2".to_string()),
                ("lat", location.latitude.to_string()),
                ("lon", location.longitude.to_string()),
                ("zoom", "10".to_string()),
                ("addressdetails", "1".to_string()),
                ("accept-language", "en".to_string()),
            ])
            .send()
            .await
            .map_err(|error| WeatherError::Location(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(WeatherError::Location(format!(
                "reverse geocoding returned {status}"
            )));
        }

        let data = response.json::<Value>().await.map_err(|error| {
            WeatherError::Location(format!(
                "failed to parse reverse geocoding response: {error}"
            ))
        })?;
        let label = parse_reverse_geocoding_label(&data)
            .ok_or_else(|| WeatherError::Location("reverse geocoding returned no label".into()))?;

        Ok(Location {
            city: label,
            ..location.clone()
        })
    }

    async fn fetch_forecast(
        &self,
        location: Location,
        config: &Config,
    ) -> Result<Snapshot, WeatherError> {
        let response = self
            .http
            .get(FORECAST_API_BASE)
            .query(&[
                ("latitude", location.latitude.to_string()),
                ("longitude", location.longitude.to_string()),
                (
                    "current",
                    "temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m,wind_direction_10m,surface_pressure,uv_index,is_day,precipitation".to_string(),
                ),
                ("hourly", "temperature_2m,weather_code,is_day".to_string()),
                (
                    "daily",
                    "weather_code,temperature_2m_max,temperature_2m_min,precipitation_sum,sunrise,sunset"
                        .to_string(),
                ),
                ("forecast_days", config.forecast_days().to_string()),
                ("timezone", "auto".to_string()),
            ])
            .send()
            .await
            .map_err(|error| WeatherError::Forecast(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(WeatherError::Forecast(format!(
                "forecast returned {status}"
            )));
        }

        let data = response.json::<Value>().await.map_err(|error| {
            WeatherError::Forecast(format!("failed to parse forecast response: {error}"))
        })?;

        Ok(parse_forecast_snapshot(
            &data,
            location,
            config.hourly_slots(),
            Local::now(),
        ))
    }
}

impl Default for WeatherClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WeatherError {
    MissingLocation,
    Location(String),
    Geocoding(String),
    Forecast(String),
}

impl std::fmt::Display for WeatherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLocation => f.write_str("no weather location configured"),
            Self::Location(message) => write!(f, "location lookup failed: {message}"),
            Self::Geocoding(message) => write!(f, "geocoding failed: {message}"),
            Self::Forecast(message) => write!(f, "forecast lookup failed: {message}"),
        }
    }
}

impl std::error::Error for WeatherError {}

pub fn configured_city(config: &Config) -> Option<String> {
    let configured = config.city_name.trim();
    if !configured.is_empty() {
        Some(configured.to_owned())
    } else {
        None
    }
}

fn parse_geocoding_location(value: &Value) -> Option<Location> {
    let first = value.get("results")?.as_array()?.first()?;
    let name = first.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }

    let country = first
        .get("country_code")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    Some(Location {
        latitude: first.get("latitude")?.as_f64()?,
        longitude: first.get("longitude")?.as_f64()?,
        city: if country.is_empty() {
            name.to_owned()
        } else {
            format!("{name}, {country}")
        },
    })
}

fn parse_reverse_geocoding_label(value: &Value) -> Option<String> {
    let address = value.get("address")?;
    let city = first_non_empty(
        address,
        &["city", "town", "village", "municipality", "county", "state"],
    );
    let country = address
        .get("country")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match (city, country) {
        (Some(city), Some(country)) => Some(format!("{city}, {country}")),
        (Some(city), None) => Some(city.to_owned()),
        (None, Some(country)) => Some(country.to_owned()),
        (None, None) => None,
    }
}

fn first_non_empty<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn parse_forecast_snapshot(
    value: &Value,
    location: Location,
    hourly_slots: usize,
    now: DateTime<Local>,
) -> Snapshot {
    Snapshot {
        current: parse_current(&value["current"]),
        hourly: parse_hourly(&value["hourly"], hourly_slots, now),
        forecast: parse_daily(&value["daily"], now),
        location,
    }
}

fn parse_current(value: &Value) -> CurrentWeather {
    let code = value["weather_code"].as_u64().unwrap_or(0) as u32;
    let is_day = value["is_day"].as_u64().unwrap_or(1) == 1;
    let wind_direction = value["wind_direction_10m"].as_f64().unwrap_or(0.0) as u16;

    CurrentWeather {
        temperature: value["temperature_2m"].as_f64().unwrap_or(0.0),
        apparent_temperature: value["apparent_temperature"].as_f64().unwrap_or(0.0),
        humidity: value["relative_humidity_2m"].as_u64().unwrap_or(0) as u8,
        weather_code: code,
        condition: wmo_condition(code).into(),
        icon: wmo_icon(code, is_day).into(),
        wind_speed: value["wind_speed_10m"].as_f64().unwrap_or(0.0),
        wind_direction,
        wind_direction_label: wind_direction_label(wind_direction).into(),
        pressure: value["surface_pressure"].as_f64().unwrap_or(0.0),
        uv_index: value["uv_index"].as_f64().unwrap_or(0.0),
        precipitation: value["precipitation"].as_f64().unwrap_or(0.0),
        is_day,
    }
}

fn parse_hourly(value: &Value, slot_count: usize, now: DateTime<Local>) -> Vec<HourlyForecast> {
    let (Some(times), Some(temperatures), Some(codes)) = (
        value["time"].as_array(),
        value["temperature_2m"].as_array(),
        value["weather_code"].as_array(),
    ) else {
        return Vec::new();
    };
    let is_days = value["is_day"].as_array();
    let current_hour = now.format("%Y-%m-%dT%H:00").to_string();
    let Some(start) = times
        .iter()
        .position(|time| time.as_str().unwrap_or("") >= current_hour.as_str())
    else {
        return Vec::new();
    };

    let mut hourly = Vec::new();
    for index in (start + 1)..times.len() {
        if hourly.len() >= slot_count {
            break;
        }

        let Some(temperature) = temperatures.get(index).and_then(Value::as_f64) else {
            continue;
        };
        let code = codes.get(index).and_then(Value::as_u64).unwrap_or(0) as u32;
        let is_day = is_days
            .and_then(|items| items.get(index))
            .and_then(Value::as_u64)
            .map(|value| value == 1)
            .unwrap_or(true);
        let time = times
            .get(index)
            .and_then(Value::as_str)
            .and_then(|value| value.split('T').nth(1))
            .unwrap_or("00:00")
            .to_owned();

        hourly.push(HourlyForecast {
            time,
            temperature,
            weather_code: code,
            condition: wmo_condition(code).into(),
            icon: wmo_icon(code, is_day).into(),
        });
    }

    hourly
}

fn parse_daily(value: &Value, now: DateTime<Local>) -> Vec<DailyForecast> {
    let (Some(dates), Some(codes), Some(maxima), Some(minima)) = (
        value["time"].as_array(),
        value["weather_code"].as_array(),
        value["temperature_2m_max"].as_array(),
        value["temperature_2m_min"].as_array(),
    ) else {
        return Vec::new();
    };
    let precipitation = value["precipitation_sum"].as_array();
    let sunrises = value["sunrise"].as_array();
    let sunsets = value["sunset"].as_array();
    let today = now.format("%Y-%m-%d").to_string();

    let mut forecast = Vec::new();
    for index in 0..dates.len() {
        let date = dates[index].as_str().unwrap_or("");
        let code = codes.get(index).and_then(Value::as_u64).unwrap_or(0) as u32;
        let is_today = date == today;

        forecast.push(DailyForecast {
            date: date.to_owned(),
            day_name: if is_today {
                "Today".into()
            } else {
                NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .map(|date| date.format("%a").to_string())
                    .unwrap_or_default()
            },
            is_today,
            weather_code: code,
            condition: wmo_condition(code).into(),
            icon: wmo_icon(code, true).into(),
            temperature_max: maxima.get(index).and_then(Value::as_f64).unwrap_or(0.0),
            temperature_min: minima.get(index).and_then(Value::as_f64).unwrap_or(0.0),
            precipitation_sum: precipitation
                .and_then(|items| items.get(index))
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            sunrise: sunrises
                .and_then(|items| items.get(index))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            sunset: sunsets
                .and_then(|items| items.get(index))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        });
    }

    forecast
}

fn wmo_condition(code: u32) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Fog",
        51 | 53 | 55 => "Drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80 | 81 | 82 => "Rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Unknown",
    }
}

fn wmo_icon(code: u32, is_day: bool) -> &'static str {
    match code {
        0 if is_day => "weather-clear-symbolic",
        0 => "weather-clear-night-symbolic",
        1 | 2 if is_day => "weather-few-clouds-symbolic",
        1 | 2 => "weather-few-clouds-night-symbolic",
        3 => "weather-overcast-symbolic",
        45 | 48 => "weather-fog-symbolic",
        51..=57 => "weather-showers-scattered-symbolic",
        61..=67 => "weather-showers-symbolic",
        71..=77 => "weather-snow-symbolic",
        80..=82 => "weather-showers-symbolic",
        85 | 86 => "weather-snow-symbolic",
        95..=99 => "weather-storm-symbolic",
        _ => "weather-overcast-symbolic",
    }
}

fn wind_direction_label(degrees: u16) -> &'static str {
    const DIRECTIONS: &[&str] = &["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let index = ((degrees as f64 + 22.5) / 45.0) as usize % DIRECTIONS.len();
    DIRECTIONS[index]
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    #[test]
    fn configured_city_uses_trimmed_explicit_city() {
        let config = Config {
            city_name: " Warsaw, PL ".into(),
            ..Config::default()
        };

        assert_eq!(configured_city(&config).as_deref(), Some("Warsaw, PL"));
    }

    #[test]
    fn configured_city_rejects_empty_config() {
        assert_eq!(configured_city(&Config::default()), None);
    }

    #[test]
    fn parse_geocoding_location_uses_first_result() {
        let location = parse_geocoding_location(&json!({
            "results": [{
                "name": "Warsaw",
                "country_code": "PL",
                "latitude": 52.2298,
                "longitude": 21.0118
            }]
        }))
        .unwrap();

        assert_eq!(location.city, "Warsaw, PL");
        assert_eq!(location.latitude, 52.2298);
        assert_eq!(location.longitude, 21.0118);
    }

    #[test]
    fn parse_geocoding_location_rejects_empty_results() {
        assert!(parse_geocoding_location(&json!({ "results": [] })).is_none());
    }

    #[test]
    fn parse_reverse_geocoding_label_prefers_city_and_country() {
        let label = parse_reverse_geocoding_label(&json!({
            "address": {
                "city": "Warsaw",
                "country": "Poland",
                "country_code": "pl"
            }
        }));

        assert_eq!(label.as_deref(), Some("Warsaw, Poland"));
    }

    #[test]
    fn parse_reverse_geocoding_label_uses_town_when_city_is_missing() {
        let label = parse_reverse_geocoding_label(&json!({
            "address": {
                "town": "Sopot",
                "country": "Poland"
            }
        }));

        assert_eq!(label.as_deref(), Some("Sopot, Poland"));
    }

    #[test]
    fn parse_current_builds_typed_current_weather() {
        let current = parse_current(&json!({
            "temperature_2m": 20.4,
            "apparent_temperature": 19.1,
            "relative_humidity_2m": 61,
            "weather_code": 2,
            "wind_speed_10m": 11.0,
            "wind_direction_10m": 90.0,
            "surface_pressure": 1013.0,
            "uv_index": 4.0,
            "is_day": 1,
            "precipitation": 0.0
        }));

        assert_eq!(current.temperature, 20.4);
        assert_eq!(current.condition, "Partly cloudy");
        assert_eq!(current.icon, "weather-few-clouds-symbolic");
        assert_eq!(current.wind_direction_label, "E");
    }

    #[test]
    fn parse_hourly_returns_requested_future_slots() {
        let now = Local.with_ymd_and_hms(2099, 1, 1, 10, 30, 0).unwrap();
        let hourly = parse_hourly(
            &json!({
                "time": [
                    "2099-01-01T10:00",
                    "2099-01-01T11:00",
                    "2099-01-01T12:00",
                    "2099-01-01T13:00",
                    "2099-01-01T14:00",
                    "2099-01-01T15:00",
                    "2099-01-01T16:00"
                ],
                "temperature_2m": [10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0],
                "weather_code": [0, 1, 2, 3, 61, 63, 80],
                "is_day": [1, 1, 1, 1, 1, 1, 1]
            }),
            5,
            now,
        );

        assert_eq!(hourly.len(), 5);
        assert_eq!(hourly[0].time, "11:00");
        assert_eq!(hourly[0].temperature, 11.0);
    }

    #[test]
    fn parse_hourly_returns_empty_when_response_is_stale() {
        let now = Local.with_ymd_and_hms(2099, 1, 1, 12, 30, 0).unwrap();
        let hourly = parse_hourly(
            &json!({
                "time": ["2099-01-01T09:00", "2099-01-01T10:00"],
                "temperature_2m": [9.0, 10.0],
                "weather_code": [0, 1],
                "is_day": [1, 1]
            }),
            5,
            now,
        );

        assert!(hourly.is_empty());
    }

    #[test]
    fn parse_daily_preserves_today_sunrise_and_sunset() {
        let now = Local.with_ymd_and_hms(2099, 1, 1, 10, 30, 0).unwrap();
        let forecast = parse_daily(
            &json!({
                "time": ["2099-01-01"],
                "weather_code": [3],
                "temperature_2m_max": [14.0],
                "temperature_2m_min": [8.0],
                "precipitation_sum": [1.5],
                "sunrise": ["2099-01-01T06:12"],
                "sunset": ["2099-01-01T19:48"]
            }),
            now,
        );

        assert_eq!(forecast.len(), 1);
        assert_eq!(forecast[0].day_name, "Today");
        assert_eq!(forecast[0].sunrise, "2099-01-01T06:12");
        assert_eq!(forecast[0].sunset, "2099-01-01T19:48");
    }

    #[test]
    fn parse_forecast_snapshot_combines_sections() {
        let now = Local.with_ymd_and_hms(2099, 1, 1, 10, 30, 0).unwrap();
        let location = Location {
            latitude: 52.2298,
            longitude: 21.0118,
            city: "Warsaw, PL".into(),
        };
        let snapshot = parse_forecast_snapshot(
            &json!({
                "current": {
                    "temperature_2m": 20.4,
                    "apparent_temperature": 19.1,
                    "relative_humidity_2m": 61,
                    "weather_code": 2,
                    "wind_speed_10m": 11.0,
                    "wind_direction_10m": 90.0,
                    "surface_pressure": 1013.0,
                    "uv_index": 4.0,
                    "is_day": 1,
                    "precipitation": 0.0
                },
                "hourly": {
                    "time": ["2099-01-01T10:00", "2099-01-01T11:00"],
                    "temperature_2m": [20.0, 21.0],
                    "weather_code": [1, 2],
                    "is_day": [1, 1]
                },
                "daily": {
                    "time": ["2099-01-01"],
                    "weather_code": [3],
                    "temperature_2m_max": [23.0],
                    "temperature_2m_min": [14.0],
                    "precipitation_sum": [1.5],
                    "sunrise": ["2099-01-01T06:12"],
                    "sunset": ["2099-01-01T19:48"]
                }
            }),
            location,
            5,
            now,
        );

        assert_eq!(snapshot.location.city, "Warsaw, PL");
        assert_eq!(snapshot.current.temperature, 20.4);
        assert_eq!(snapshot.hourly.len(), 1);
        assert_eq!(snapshot.forecast.len(), 1);
    }
}
