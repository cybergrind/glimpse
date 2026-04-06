use std::time::Duration;

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use super::config::WeatherConfig;
use super::popover::{WeatherPopover, WeatherPopoverInit, WeatherPopoverInput};

const GEOCODE_API_BASE: &str = "https://geocoding-api.open-meteo.com/v1/search";
const FORECAST_API_BASE: &str = "https://api.open-meteo.com/v1/forecast";

#[derive(Debug, Clone, serde::Serialize, Default)]
pub(crate) struct WeatherCurrent {
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

#[derive(Debug, Clone, serde::Serialize, Default)]
pub(crate) struct WeatherHourly {
    pub time: String,
    pub temperature: f64,
    pub weather_code: u32,
    pub condition: String,
    pub icon: String,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
pub(crate) struct WeatherDaily {
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

#[derive(Debug, Clone, serde::Serialize, Default)]
pub(crate) struct WeatherLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub city: String,
}

#[derive(Debug)]
struct WeatherSnapshot {
    current: WeatherCurrent,
    hourly: Vec<WeatherHourly>,
    forecast: Vec<WeatherDaily>,
    location: WeatherLocation,
}

pub struct Weather {
    config: WeatherConfig,
    icon_name: String,
    label: String,
    tooltip: String,
    popover: Controller<WeatherPopover>,
}

#[derive(Debug)]
pub enum WeatherMsg {
    CurrentUpdate(WeatherCurrent),
    HourlyUpdate(Vec<WeatherHourly>),
    ForecastUpdate(Vec<WeatherDaily>),
    LocationUpdate(WeatherLocation),
    TogglePopover,
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Weather {
    type Init = WeatherConfig;
    type Input = WeatherMsg;
    type Output = ();
    type CommandOutput = WeatherMsg;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "weather",
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(WeatherMsg::TogglePopover);
                }
            },

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 16,
            },

            gtk::Label {
                #[watch]
                set_label: &model.label,
                add_css_class: "weather-label",
                #[watch]
                set_visible: !model.label.is_empty(),
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = WeatherPopover::builder()
            .launch(WeatherPopoverInit {
                parent: root.clone(),
                hourly_slots: init.hourly_slots,
                forecast_days: init.forecast_days,
            })
            .detach();

        let config = init.clone();
        let refresh_interval = init.refresh_interval;

        let model = Weather {
            config: init,
            icon_name: "weather-overcast-symbolic".into(),
            label: String::new(),
            tooltip: String::new(),
            popover,
        };

        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("weather applet: starting local fetch loop");
                    let http = reqwest::Client::new();
                    let Some(city_name) = resolve_city_name(&config, ip_geolocate_city).await else {
                        tracing::warn!("weather applet: no city configured and fallback disabled");
                        return;
                    };
                    let Some((latitude, longitude, city)) = geocode_city(&http, &city_name).await else {
                        let _ = out.send(WeatherMsg::Unavailable);
                        return;
                    };
                    tracing::info!(lat = latitude, lon = longitude, city = %city, "weather applet: resolved location");
                    loop {
                        match fetch_weather_snapshot(
                            &http,
                            latitude,
                            longitude,
                            &city,
                            config.hourly_slots,
                        )
                        .await
                        {
                            Ok(snapshot) => {
                                let _ = out.send(WeatherMsg::CurrentUpdate(snapshot.current));
                                let _ = out.send(WeatherMsg::HourlyUpdate(snapshot.hourly));
                                let _ = out.send(WeatherMsg::ForecastUpdate(snapshot.forecast));
                                let _ = out.send(WeatherMsg::LocationUpdate(snapshot.location));
                            }
                            Err(error) => {
                                tracing::warn!(%error, "weather applet: forecast fetch failed");
                                let _ = out.send(WeatherMsg::Unavailable);
                            }
                        }
                        tokio::time::sleep(Duration::from_secs(refresh_interval.max(60))).await;
                    }
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            WeatherMsg::CurrentUpdate(data) => {
                let temp = data.temperature;
                let condition = data.condition.as_str();
                let icon = data.icon.as_str();
                let feels = data.apparent_temperature;

                self.icon_name = icon.to_owned();
                self.label = self
                    .config
                    .label_format
                    .replace("{temp}", &format!("{temp:.0}"))
                    .replace("{condition}", condition)
                    .replace("{feels_like}", &format!("{feels:.0}"));
                self.tooltip = self
                    .config
                    .tooltip_format
                    .replace("{temp}", &format!("{temp:.0}"))
                    .replace("{condition}", condition)
                    .replace("{feels_like}", &format!("{feels:.0}"));

                tracing::info!(temp, condition, "weather applet: current update");
                self.popover.emit(WeatherPopoverInput::UpdateCurrent(data));
            }
            WeatherMsg::HourlyUpdate(data) => {
                self.popover.emit(WeatherPopoverInput::UpdateHourly(data));
            }
            WeatherMsg::ForecastUpdate(data) => {
                self.popover.emit(WeatherPopoverInput::UpdateForecast(data));
            }
            WeatherMsg::LocationUpdate(data) => {
                self.popover.emit(WeatherPopoverInput::UpdateLocation(data));
            }
            WeatherMsg::TogglePopover => {
                self.popover.emit(WeatherPopoverInput::Toggle);
            }
            WeatherMsg::Unavailable => {
                tracing::warn!("weather applet: weather data unavailable");
            }
        }
    }
}

async fn resolve_city_name<F, Fut>(config: &WeatherConfig, ip_lookup: F) -> Option<String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Option<String>>,
{
    let configured = config.city_name.trim();
    if !configured.is_empty() {
        return Some(configured.to_owned());
    }

    if !config.geolocate {
        return None;
    }

    ip_lookup().await.and_then(|city| {
        let trimmed = city.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

async fn ip_geolocate_city() -> Option<String> {
    let response = match reqwest::get("https://ipapi.co/json/").await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%error, "weather applet: ip geolocation request failed");
            return None;
        }
    };
    let resp: serde_json::Value = match response.json().await {
        Ok(resp) => resp,
        Err(error) => {
            tracing::warn!(%error, "weather applet: ip geolocation response parse failed");
            return None;
        }
    };
    let city = resp["city"].as_str()?.trim();
    if city.is_empty() {
        return None;
    }
    let country = resp["country_code"].as_str().unwrap_or("").trim();
    Some(if country.is_empty() {
        city.to_owned()
    } else {
        format!("{city}, {country}")
    })
}

async fn geocode_city(http: &reqwest::Client, city: &str) -> Option<(f64, f64, String)> {
    let response = match http
        .get(GEOCODE_API_BASE)
        .query(&[
            ("name", city),
            ("count", "1"),
            ("language", "en"),
            ("format", "json"),
        ])
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(city = %city, %error, "weather applet: geocoding request failed");
            return None;
        }
    };
    if !response.status().is_success() {
        tracing::warn!(city = %city, status = %response.status(), "weather applet: geocoding returned non-success status");
        return None;
    }
    let data: serde_json::Value = match response.json().await {
        Ok(data) => data,
        Err(error) => {
            tracing::warn!(city = %city, %error, "weather applet: geocoding response parse failed");
            return None;
        }
    };
    let location = parse_geocoding_location(&data);
    if location.is_none() {
        tracing::warn!(city = %city, "weather applet: geocoding returned no usable result");
    }
    location
}

fn parse_geocoding_location(data: &serde_json::Value) -> Option<(f64, f64, String)> {
    let first = data["results"].as_array()?.first()?;
    let name = first["name"].as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let latitude = first["latitude"].as_f64()?;
    let longitude = first["longitude"].as_f64()?;
    let country = first["country_code"].as_str().unwrap_or("").trim();
    let city = if country.is_empty() {
        name.to_owned()
    } else {
        format!("{name}, {country}")
    };
    Some((latitude, longitude, city))
}

async fn fetch_weather_snapshot(
    http: &reqwest::Client,
    latitude: f64,
    longitude: f64,
    city: &str,
    hourly_slots: usize,
) -> Result<WeatherSnapshot, String> {
    let response = http
        .get(FORECAST_API_BASE)
        .query(&[
            ("latitude", latitude.to_string()),
            ("longitude", longitude.to_string()),
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
            ("forecast_days", "10".to_string()),
            ("timezone", "auto".to_string()),
        ])
        .send()
        .await
        .map_err(|error| {
            tracing::warn!(lat = latitude, lon = longitude, city = %city, %error, "weather applet: forecast request failed");
            error.to_string()
        })?;
    let status = response.status();
    if !status.is_success() {
        tracing::warn!(lat = latitude, lon = longitude, city = %city, %status, "weather applet: forecast returned non-success status");
        return Err(format!("forecast request failed with status {status}"));
    }
    let data: serde_json::Value = response.json().await.map_err(|error| {
        tracing::warn!(lat = latitude, lon = longitude, city = %city, %error, "weather applet: forecast response parse failed");
        error.to_string()
    })?;

    let current = parse_current(&data["current"]);
    let hourly = parse_hourly(&data["hourly"], hourly_slots);
    let forecast = parse_daily(&data["daily"]);
    let location = WeatherLocation {
        latitude,
        longitude,
        city: city.to_owned(),
    };

    Ok(WeatherSnapshot {
        current,
        hourly,
        forecast,
        location,
    })
}

fn parse_current(c: &serde_json::Value) -> WeatherCurrent {
    let code = c["weather_code"].as_u64().unwrap_or(0) as u32;
    let is_day = c["is_day"].as_u64().unwrap_or(1) == 1;
    let wind_dir = c["wind_direction_10m"].as_f64().unwrap_or(0.0) as u16;

    WeatherCurrent {
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
    }
}

fn parse_hourly(h: &serde_json::Value, slot_count: usize) -> Vec<WeatherHourly> {
    let mut hourly = Vec::new();
    let times = h["time"].as_array();
    let temps = h["temperature_2m"].as_array();
    let codes = h["weather_code"].as_array();
    let is_days = h["is_day"].as_array();

    let (Some(times), Some(temps), Some(codes)) = (times, temps, codes) else {
        return hourly;
    };

    let now = chrono::Local::now();
    let current_hour = now.format("%Y-%m-%dT%H:00").to_string();
    let start = times
        .iter()
        .position(|t| t.as_str().unwrap_or("") >= current_hour.as_str())
        .unwrap_or(0);

    let count = slot_count.min(8);

    for offset in 1..=count {
        let i = start + offset;
        if i >= times.len() {
            break;
        }
        let time_str = times[i].as_str().unwrap_or("");
        let code = codes[i].as_u64().unwrap_or(0) as u32;
        let is_day = is_days
            .and_then(|a| a.get(i))
            .and_then(|v| v.as_u64())
            .map(|v| v == 1)
            .unwrap_or(true);
        let label = time_str.split('T').nth(1).unwrap_or("00:00").to_owned();

        hourly.push(WeatherHourly {
            time: label,
            temperature: temps[i].as_f64().unwrap_or(0.0),
            weather_code: code,
            condition: wmo_condition(code),
            icon: wmo_icon(code, is_day),
        });
    }

    hourly
}

fn parse_daily(d: &serde_json::Value) -> Vec<WeatherDaily> {
    let mut forecast = Vec::new();
    let dates = d["time"].as_array();
    let codes = d["weather_code"].as_array();
    let maxs = d["temperature_2m_max"].as_array();
    let mins = d["temperature_2m_min"].as_array();
    let precips = d["precipitation_sum"].as_array();
    let sunrises = d["sunrise"].as_array();
    let sunsets = d["sunset"].as_array();

    let (Some(dates), Some(codes), Some(maxs), Some(mins)) = (dates, codes, maxs, mins) else {
        return forecast;
    };

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
        let sunrise = sunrises
            .and_then(|a| a.get(i))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        let sunset = sunsets
            .and_then(|a| a.get(i))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();

        forecast.push(WeatherDaily {
            date: date.to_owned(),
            day_name,
            is_today,
            weather_code: code,
            condition: wmo_condition(code),
            icon: wmo_icon(code, true),
            temperature_max: maxs[i].as_f64().unwrap_or(0.0),
            temperature_min: mins[i].as_f64().unwrap_or(0.0),
            precipitation_sum,
            sunrise,
            sunset,
        });
    }

    forecast
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
    }
    .to_owned()
}

fn wmo_icon(code: u32, is_day: bool) -> String {
    let icon = match code {
        0 => {
            if is_day {
                "weather-clear-symbolic"
            } else {
                "weather-clear-night-symbolic"
            }
        }
        1 | 2 => {
            if is_day {
                "weather-few-clouds-symbolic"
            } else {
                "weather-few-clouds-night-symbolic"
            }
        }
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

#[cfg(test)]
mod tests {
    use super::{
        parse_current, parse_daily, parse_geocoding_location, parse_hourly, resolve_city_name,
        WeatherSnapshot,
    };
    use crate::applets::weather::config::WeatherConfig;

    #[tokio::test]
    async fn resolve_location_uses_explicit_city_without_ip_lookup() {
        let cfg = WeatherConfig {
            city_name: "Warsaw, PL".into(),
            geolocate: true,
            ..WeatherConfig::default()
        };

        let resolved = resolve_city_name(&cfg, || async {
            panic!("ip lookup should not be used when city is configured");
            #[allow(unreachable_code)]
            Some("ignored".to_string())
        })
        .await;

        assert_eq!(resolved.as_deref(), Some("Warsaw, PL"));
    }

    #[tokio::test]
    async fn resolve_location_returns_none_when_city_unset_and_fallback_disabled() {
        let cfg = WeatherConfig::default();
        let resolved = resolve_city_name(&cfg, || async { Some("Ignored, PL".to_string()) }).await;
        assert_eq!(resolved, None);
    }

    #[tokio::test]
    async fn resolve_location_uses_ip_when_city_unset_and_fallback_enabled() {
        let cfg = WeatherConfig {
            geolocate: true,
            ..WeatherConfig::default()
        };
        let resolved = resolve_city_name(&cfg, || async { Some("Warsaw, PL".to_string()) }).await;
        assert_eq!(resolved.as_deref(), Some("Warsaw, PL"));
    }

    #[test]
    fn parse_geocoding_result_uses_first_match() {
        let data = serde_json::json!({
            "results": [{
                "name": "Warsaw",
                "country_code": "PL",
                "latitude": 52.2298,
                "longitude": 21.0118
            }]
        });

        let (latitude, longitude, city) = parse_geocoding_location(&data).unwrap();
        assert_eq!(city, "Warsaw, PL");
        assert_eq!(latitude, 52.2298);
        assert_eq!(longitude, 21.0118);
    }

    #[test]
    fn parse_geocoding_result_rejects_empty_results() {
        let data = serde_json::json!({ "results": [] });
        assert!(parse_geocoding_location(&data).is_none());
    }

    #[test]
    fn weather_snapshot_uses_typed_sections() {
        let current = parse_current(&serde_json::json!({
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
        let hourly = parse_hourly(&serde_json::json!({
            "time": ["2099-01-01T10:00", "2099-01-01T11:00", "2099-01-01T12:00"],
            "temperature_2m": [20.0, 21.0, 22.0],
            "weather_code": [1, 2, 3],
            "is_day": [1, 1, 1]
        }), 5);
        let forecast = parse_daily(&serde_json::json!({
            "time": ["2099-01-01"],
            "weather_code": [3],
            "temperature_2m_max": [23.0],
            "temperature_2m_min": [14.0],
            "precipitation_sum": [1.5],
            "sunrise": ["2099-01-01T06:12"],
            "sunset": ["2099-01-01T19:48"]
        }));

        let snapshot = WeatherSnapshot {
            current,
            hourly,
            forecast,
            location: super::WeatherLocation {
                latitude: 52.2298,
                longitude: 21.0118,
                city: "Warsaw, PL".into(),
            },
        };

        assert_eq!(snapshot.current.temperature, 20.4);
        assert_eq!(snapshot.location.city, "Warsaw, PL");
    }

    #[test]
    fn parse_hourly_returns_five_future_slots() {
        let data = serde_json::json!({
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
        });

        let hourly = parse_hourly(&data, 5);

        assert_eq!(hourly.len(), 5);
    }

    #[test]
    fn parse_daily_preserves_today_sunrise_and_sunset() {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let data = serde_json::json!({
            "time": [today],
            "weather_code": [3],
            "temperature_2m_max": [14.0],
            "temperature_2m_min": [8.0],
            "precipitation_sum": [1.5],
            "sunrise": [format!("{}T06:12", today)],
            "sunset": [format!("{}T19:48", today)]
        });

        let forecast = parse_daily(&data);

        assert_eq!(forecast.len(), 1);
        assert_eq!(forecast[0].sunrise, format!("{}T06:12", today));
        assert_eq!(forecast[0].sunset, format!("{}T19:48", today));
    }
}
