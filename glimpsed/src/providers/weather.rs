use std::pin::Pin;

use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "weather";
const TOPICS: &[&str] = &[
    "weather.current",
    "weather.hourly",
    "weather.forecast",
    "weather.location",
];
const METHODS: &[&str] = &["weather.refresh", "weather.set_location"];

const API_BASE: &str = "https://api.open-meteo.com/v1/forecast";

#[derive(Debug, Clone, Serialize, Default)]
struct WeatherCurrent {
    temperature: f64,
    apparent_temperature: f64,
    humidity: u8,
    weather_code: u32,
    condition: String,
    icon: String,
    wind_speed: f64,
    wind_direction: u16,
    wind_direction_label: String,
    pressure: f64,
    uv_index: f64,
    precipitation: f64,
    is_day: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
struct WeatherHourly {
    time: String,
    temperature: f64,
    weather_code: u32,
    condition: String,
    icon: String,
}

#[derive(Debug, Clone, Serialize, Default)]
struct WeatherDaily {
    date: String,
    day_name: String,
    is_today: bool,
    weather_code: u32,
    condition: String,
    icon: String,
    temperature_max: f64,
    temperature_min: f64,
    precipitation_sum: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
struct WeatherLocation {
    latitude: f64,
    longitude: f64,
    city: String,
}

struct WeatherProvider {
    current: WeatherCurrent,
    hourly: Vec<WeatherHourly>,
    forecast: Vec<WeatherDaily>,
    location: WeatherLocation,
    refresh_secs: u64,
    http: reqwest::Client,
}

impl Provider for WeatherProvider {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }

    fn run(
        &mut self,
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            tracing::info!("weather: starting");

            let has_location = self.location.latitude != 0.0 || self.location.longitude != 0.0;
            if has_location {
                tracing::info!(
                    lat = self.location.latitude,
                    lon = self.location.longitude,
                    city = %self.location.city,
                    "weather: using configured location"
                );
                self.fetch().await;
                self.emit_all(&events).await;
            } else {
                tracing::info!("weather: waiting for location via weather.set_location");
            }

            let initial_delay = if has_location {
                std::time::Duration::from_secs(self.refresh_secs)
            } else {
                std::time::Duration::from_secs(86400)
            };
            let refresh = tokio::time::sleep(initial_delay);
            tokio::pin!(refresh);

            loop {
                let interval = std::time::Duration::from_secs(self.refresh_secs);
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        let location_changed = self.handle_request(req, &events).await;
                        if location_changed {
                            let interval = std::time::Duration::from_secs(self.refresh_secs);
                            refresh.as_mut().reset(tokio::time::Instant::now() + interval);
                        }
                    }
                    () = &mut refresh => {
                        if self.location.latitude != 0.0 || self.location.longitude != 0.0 {
                            tracing::info!("weather: refreshing");
                            self.fetch().await;
                            self.emit_all(&events).await;
                        }
                        refresh.as_mut().reset(tokio::time::Instant::now() + interval);
                    }
                }
            }
            Ok(())
        })
    }
}

impl WeatherProvider {
    async fn fetch(&mut self) {
        let url = format!(
            "{}?latitude={}&longitude={}\
             &current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,\
             wind_speed_10m,wind_direction_10m,surface_pressure,uv_index,is_day,precipitation\
             &hourly=temperature_2m,weather_code,is_day\
             &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_sum\
             &forecast_days=10&timezone=auto",
            API_BASE, self.location.latitude, self.location.longitude,
        );

        let resp = match self.http.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("weather: fetch failed: {e}");
                return;
            }
        };

        let data: serde_json::Value = match resp.json().await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("weather: parse failed: {e}");
                return;
            }
        };

        self.parse_current(&data["current"]);
        self.parse_hourly(&data["hourly"]);
        self.parse_daily(&data["daily"]);
    }

    fn parse_current(&mut self, c: &serde_json::Value) {
        let code = c["weather_code"].as_u64().unwrap_or(0) as u32;
        let is_day = c["is_day"].as_u64().unwrap_or(1) == 1;
        let wind_dir = c["wind_direction_10m"].as_f64().unwrap_or(0.0) as u16;

        self.current = WeatherCurrent {
            temperature: c["temperature_2m"].as_f64().unwrap_or(0.0),
            apparent_temperature: c["apparent_temperature"].as_f64().unwrap_or(0.0),
            humidity: c["relative_humidity_2m"].as_u64().unwrap_or(0) as u8,
            weather_code: code,
            condition: wmo_condition(code),
            icon: wmo_icon(code, is_day),
            wind_speed: c["wind_speed_10m"].as_f64().unwrap_or(0.0),
            wind_direction: wind_dir,
            wind_direction_label: wind_direction_label(wind_dir),
            pressure: c["surface_pressure"].as_f64().unwrap_or(0.0),
            uv_index: c["uv_index"].as_f64().unwrap_or(0.0),
            precipitation: c["precipitation"].as_f64().unwrap_or(0.0),
            is_day,
        };
    }

    fn parse_hourly(&mut self, h: &serde_json::Value) {
        self.hourly.clear();
        let times = h["time"].as_array();
        let temps = h["temperature_2m"].as_array();
        let codes = h["weather_code"].as_array();
        let is_days = h["is_day"].as_array();

        let (Some(times), Some(temps), Some(codes)) = (times, temps, codes) else { return };

        // Find current hour index, take next 24.
        let now = chrono::Local::now();
        let current_hour = now.format("%Y-%m-%dT%H:00").to_string();
        let start = times.iter()
            .position(|t| t.as_str().unwrap_or("") >= current_hour.as_str())
            .unwrap_or(0);

        for &offset in &[1, 3, 6, 9, 12] {
            let i = start + offset;
            if i >= times.len() { break; }
            let time_str = times[i].as_str().unwrap_or("");
            let code = codes[i].as_u64().unwrap_or(0) as u32;
            let is_day = is_days.and_then(|a| a.get(i))
                .and_then(|v| v.as_u64())
                .map(|v| v == 1)
                .unwrap_or(true);
            let label = time_str.split('T').nth(1).unwrap_or("00:00").to_owned();

            self.hourly.push(WeatherHourly {
                time: label,
                temperature: temps[i].as_f64().unwrap_or(0.0),
                weather_code: code,
                condition: wmo_condition(code),
                icon: wmo_icon(code, is_day),
            });
        }
    }

    fn parse_daily(&mut self, d: &serde_json::Value) {
        self.forecast.clear();
        let dates = d["time"].as_array();
        let codes = d["weather_code"].as_array();
        let maxs = d["temperature_2m_max"].as_array();
        let mins = d["temperature_2m_min"].as_array();
        let precips = d["precipitation_sum"].as_array();

        let (Some(dates), Some(codes), Some(maxs), Some(mins)) = (dates, codes, maxs, mins)
            else { return };

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        for i in 0..dates.len() {
            let date = dates[i].as_str().unwrap_or("");
            let code = codes[i].as_u64().unwrap_or(0) as u32;
            let is_today = date == today;
            let day_name = if is_today {
                "Today".into()
            } else {
                chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .map(|d| d.format("%a").to_string())
                    .unwrap_or_default()
            };
            let precipitation_sum = precips
                .and_then(|a| a.get(i))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            self.forecast.push(WeatherDaily {
                date: date.to_owned(),
                day_name,
                is_today,
                weather_code: code,
                condition: wmo_condition(code),
                icon: wmo_icon(code, true),
                temperature_max: maxs[i].as_f64().unwrap_or(0.0),
                temperature_min: mins[i].as_f64().unwrap_or(0.0),
                precipitation_sum,
            });
        }
    }

    async fn emit_all(&self, events: &mpsc::Sender<ProviderEvent>) {
        let _ = events.send(ProviderEvent {
            topic: "weather.current".into(),
            data: serde_json::to_value(&self.current).unwrap_or_default(),
        }).await;
        let _ = events.send(ProviderEvent {
            topic: "weather.hourly".into(),
            data: serde_json::to_value(&self.hourly).unwrap_or_default(),
        }).await;
        let _ = events.send(ProviderEvent {
            topic: "weather.forecast".into(),
            data: serde_json::to_value(&self.forecast).unwrap_or_default(),
        }).await;
        let _ = events.send(ProviderEvent {
            topic: "weather.location".into(),
            data: serde_json::to_value(&self.location).unwrap_or_default(),
        }).await;
    }

    /// Returns true if location was changed.
    async fn handle_request(&mut self, req: ProviderRequest, events: &mpsc::Sender<ProviderEvent>) -> bool {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "weather.current" => serde_json::to_value(&self.current).ok(),
                    "weather.hourly" => serde_json::to_value(&self.hourly).ok(),
                    "weather.forecast" => serde_json::to_value(&self.forecast).ok(),
                    "weather.location" => serde_json::to_value(&self.location).ok(),
                    _ => None,
                };
                let _ = reply.send(data);
                false
            }
            ProviderRequest::Call { method, params, reply } => {
                let mut location_changed = false;
                let result = match method.as_str() {
                    "weather.refresh" => {
                        tracing::info!("weather: manual refresh");
                        self.fetch().await;
                        self.emit_all(events).await;
                        Ok(json!(null))
                    }
                    "weather.set_location" => {
                        let lat = params["latitude"].as_f64();
                        let lon = params["longitude"].as_f64();
                        let city = params["city"].as_str().unwrap_or("").to_owned();
                        if let Some(interval) = params["refresh_interval"].as_u64() {
                            if interval >= 60 {
                                self.refresh_secs = interval;
                            }
                        }
                        match (lat, lon) {
                            (Some(lat), Some(lon)) => {
                                tracing::info!(lat, lon, city = %city, refresh = self.refresh_secs, "weather: location set");
                                self.location = WeatherLocation { latitude: lat, longitude: lon, city };
                                self.fetch().await;
                                self.emit_all(events).await;
                                location_changed = true;
                                Ok(json!(null))
                            }
                            _ => Err(anyhow::anyhow!("missing 'latitude' and/or 'longitude'")),
                        }
                    }
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                let _ = reply.send(result);
                location_changed
            }
        }
    }
}

fn wmo_condition(code: u32) -> String {
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
    }.to_owned()
}

fn wmo_icon(code: u32, is_day: bool) -> String {
    let icon = match code {
        0 => if is_day { "weather-clear-symbolic" } else { "weather-clear-night-symbolic" },
        1 | 2 => if is_day { "weather-few-clouds-symbolic" } else { "weather-few-clouds-night-symbolic" },
        3 => "weather-overcast-symbolic",
        45 | 48 => "weather-fog-symbolic",
        51..=57 => "weather-showers-scattered-symbolic",
        61..=67 => "weather-showers-symbolic",
        71..=77 => "weather-snow-symbolic",
        80..=82 => "weather-showers-symbolic",
        85 | 86 => "weather-snow-symbolic",
        95..=99 => "weather-storm-symbolic",
        _ => "weather-overcast-symbolic",
    };
    icon.to_owned()
}

fn wind_direction_label(degrees: u16) -> String {
    const DIRS: &[&str] = &["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    let idx = ((degrees as f64 + 22.5) / 45.0) as usize % 8;
    DIRS[idx].to_owned()
}

pub struct WeatherProviderFactory;

impl ProviderFactory for WeatherProviderFactory {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }
    fn create(&self) -> Box<dyn Provider> {
        Box::new(WeatherProvider {
            current: WeatherCurrent::default(),
            hourly: Vec::new(),
            forecast: Vec::new(),
            location: WeatherLocation::default(),
            refresh_secs: 1800,
            http: reqwest::Client::new(),
        })
    }
}
