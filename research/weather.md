# Weather Provider

**Source:** Open-Meteo HTTP API (free, no API key), geolocation provider for coordinates

**What it does:** Fetches current weather, hourly forecast, and daily forecast with weather condition codes and icons.

## System Interface

### Open-Meteo API

Base URL: `https://api.open-meteo.com/v1/forecast`

Request:
```
GET /v1/forecast?latitude=47.42&longitude=9.37
  &current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,
           wind_speed_10m,wind_direction_10m,surface_pressure,uv_index,is_day
  &hourly=temperature_2m,weather_code,is_day
  &daily=weather_code,temperature_2m_max,temperature_2m_min
  &forecast_days=10
  &timezone=auto
```

Response (JSON):
```json
{
  "current": {
    "time": "2026-04-04T12:00",
    "temperature_2m": 15.3,
    "relative_humidity_2m": 65,
    "apparent_temperature": 13.1,
    "weather_code": 3,
    "wind_speed_10m": 12.5,
    "wind_direction_10m": 225,
    "surface_pressure": 1013.2,
    "uv_index": 4.5,
    "is_day": 1
  },
  "hourly": {
    "time": ["2026-04-04T12:00", "2026-04-04T13:00", ...],
    "temperature_2m": [15.3, 16.1, ...],
    "weather_code": [3, 2, ...],
    "is_day": [1, 1, ...]
  },
  "daily": {
    "time": ["2026-04-04", "2026-04-05", ...],
    "weather_code": [3, 61, ...],
    "temperature_2m_max": [18.5, 15.2, ...],
    "temperature_2m_min": [8.3, 7.1, ...]
  }
}
```

### WMO Weather Codes

- 0 = Clear sky
- 1 = Mainly clear, 2 = Partly cloudy, 3 = Overcast
- 45 = Fog, 48 = Depositing rime fog
- 51 = Light drizzle, 53 = Moderate drizzle, 55 = Dense drizzle
- 56 = Light freezing drizzle, 57 = Dense freezing drizzle
- 61 = Slight rain, 63 = Moderate rain, 65 = Heavy rain
- 66 = Light freezing rain, 67 = Heavy freezing rain
- 71 = Slight snow, 73 = Moderate snow, 75 = Heavy snow
- 77 = Snow grains
- 80 = Slight rain showers, 81 = Moderate rain showers, 82 = Violent rain showers
- 85 = Slight snow showers, 86 = Heavy snow showers
- 95 = Thunderstorm, 96 = Thunderstorm with slight hail, 99 = Thunderstorm with heavy hail

## Topics

- `weather.current` — current conditions + stats
- `weather.hourly` — next 24h (panel displays 6: now + next 5)
- `weather.forecast` — 10-day daily forecast
- `weather.location` — location used for forecast

## Methods

- `weather.refresh` — force re-fetch

## Location Resolution

Priority:
1. Config: `latitude`/`longitude` in panel.toml
2. GeoClue D-Bus (`org.freedesktop.GeoClue2`) — automatic
3. Fallback: IP-based via `https://ipapi.co/json/`

## Types

```rust
struct WeatherCurrent {
    temperature: f64,
    apparent_temperature: f64,
    humidity: u8,
    weather_code: u32,
    condition: String,       // "Clear sky", "Partly cloudy", etc.
    icon: String,            // Adwaita symbolic icon name
    wind_speed: f64,         // km/h
    wind_direction: u16,     // degrees (0=N, 90=E, 180=S, 270=W)
    wind_direction_label: String, // "N", "NE", "NW", etc.
    pressure: f64,           // hPa
    uv_index: f64,
    is_day: bool,
    time: String,
}

struct WeatherHourly {
    time: String,            // "13:00"
    temperature: f64,
    weather_code: u32,
    condition: String,
    icon: String,
    is_day: bool,
}

struct WeatherDaily {
    date: String,            // "2026-04-05"
    day_name: String,        // "Mon", "Tue", etc.
    weather_code: u32,
    condition: String,
    icon: String,
    temperature_max: f64,
    temperature_min: f64,
}

struct WeatherLocation {
    latitude: f64,
    longitude: f64,
    city: String,
}
```

## Icons

WMO code → Adwaita icon:

Day:
- 0 → `weather-clear-symbolic`
- 1, 2 → `weather-few-clouds-symbolic`
- 3 → `weather-overcast-symbolic`
- 45, 48 → `weather-fog-symbolic`
- 51–57 → `weather-showers-scattered-symbolic`
- 61–67 → `weather-showers-symbolic`
- 71–77 → `weather-snow-symbolic`
- 80–82 → `weather-showers-symbolic`
- 85–86 → `weather-snow-symbolic`
- 95–99 → `weather-storm-symbolic`

Night (is_day=false):
- 0 → `weather-clear-night-symbolic`
- 1, 2 → `weather-few-clouds-night-symbolic`
- All others same as day

## Panel Applet

```
(☀️) 22°
```

Config:
```toml
[applets.weather]
extends = "weather"
latitude = 47.42
longitude = 9.37
city_name = "St. Gallen"
units = "celsius"              # celsius | fahrenheit
label_format = "{temp}°"       # {temp}, {condition}, {feels_like}
refresh_interval = 1800        # seconds (30min)
```

## Popover Layout

```
┌──────────────────────────────────────────┐
│  (☀️ 32px)  Weather                      │
│  St. Gallen · Partly Cloudy · 22°        │  ← hero
├──────────────────────────────────────────┤
│  Now   13h   14h   15h   16h   17h       │  ← 4h forecast
│   ☀️    🌤    🌤    ⛅    ⛅    🌧       │     (current + next 5h)
│  22°   21°   20°   19°   18°   17°       │
├──────────────────────────────────────────┤
│  Feels like  24°        Humidity  65%    │  ← stats (2-column)
│  Wind  12 km/h NW       UV Index  5     │
│  Pressure  1013 hPa                      │
├──────────────────────────────────────────┤
│  Mon    ☀️   18° / 26°                    │  ← 10-day forecast
│  Tue    🌤   16° / 23°                    │
│  Wed    🌧   14° / 19°                    │
│  Thu    ⛅   15° / 22°                    │
│  Fri    ☀️   17° / 25°                    │
│  Sat    🌤   16° / 24°                    │
│  Sun    🌧   13° / 18°                    │
│  Mon    ☀️   17° / 25°                    │
│  Tue    🌤   15° / 23°                    │
│  Wed    ⛅   14° / 21°                    │
└──────────────────────────────────────────┘
```

## File Structure

```
glimpsed/src/providers/weather.rs    — provider
glimpse-panel/src/applets/weather/
  applet.rs                          — panel icon + temp
  popover.rs                         — hero + hourly + stats + daily
  config.rs                          — settings
  mod.rs
```

## Crates

- `reqwest` — HTTP client (blocking=false, json feature)
- `serde` / `serde_json` — JSON parsing

## Notes

- Open-Meteo is free, no API key — ideal for open source
- Use `timezone=auto` for local times
- Cache responses, refresh every 30min
- Night icons between sunset and sunrise (use `is_day` field)
- Provider runs a timer loop, not event-driven
- Location from config is simplest; GeoClue integration is future work
