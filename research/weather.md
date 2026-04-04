# Weather Provider

**Source:** Open-Meteo HTTP API (free, no API key), geolocation provider for coordinates

**What it does:** Fetches current weather, hourly forecast (4h), and daily forecast (10d) with weather condition codes and icons.

## System Interface

### Open-Meteo API

Base URL: `https://api.open-meteo.com/v1/forecast`

Example request:
```
GET /v1/forecast?latitude=52.52&longitude=13.41&current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m,wind_direction_10m,surface_pressure&hourly=temperature_2m,weather_code,precipitation_probability&daily=weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,uv_index_max,precipitation_sum&timezone=auto&forecast_hours=4&forecast_days=10
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
    "surface_pressure": 1013.2
  },
  "hourly": {
    "time": ["2026-04-04T12:00", "2026-04-04T13:00", ...],
    "temperature_2m": [15.3, 16.1, ...],
    "weather_code": [3, 2, ...],
    "precipitation_probability": [10, 20, ...]
  },
  "daily": {
    "time": ["2026-04-04", "2026-04-05", ...],
    "weather_code": [3, 61, ...],
    "temperature_2m_max": [18.5, 15.2, ...],
    "temperature_2m_min": [8.3, 7.1, ...],
    "sunrise": ["2026-04-04T06:15", ...],
    "sunset": ["2026-04-04T19:45", ...],
    "uv_index_max": [4.5, 3.2, ...],
    "precipitation_sum": [0.0, 5.2, ...]
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

- `weather.current` — current conditions
- `weather.forecast_hourly` — next 4 hours
- `weather.forecast_daily` — next 10 days
- `weather.location` — location used for forecast

## Methods

- `weather.refresh()` — force fetch new data
- `weather.set_location(latitude: f64, longitude: f64, city: Option<String>)` — override location
- `weather.set_units(temperature: TempUnit, wind: WindUnit)` — set display units

## Types

```rust
/// Temperature unit
enum TempUnit {
    Celsius,
    Fahrenheit,
}

/// Wind speed unit
enum WindUnit {
    Kmh,
    Mph,
    Ms,
    Knots,
}

/// WMO weather condition
enum WeatherCondition {
    ClearSky,
    MainlyClear,
    PartlyCloudy,
    Overcast,
    Fog,
    Drizzle { intensity: Intensity },
    FreezingDrizzle { intensity: Intensity },
    Rain { intensity: Intensity },
    FreezingRain { intensity: Intensity },
    Snow { intensity: Intensity },
    SnowGrains,
    RainShowers { intensity: Intensity },
    SnowShowers { intensity: Intensity },
    Thunderstorm,
    ThunderstormHail { intensity: Intensity },
}

enum Intensity {
    Slight,
    Moderate,
    Heavy,
}

/// Current weather, emitted on `weather.current`
struct WeatherCurrent {
    /// Temperature in configured units
    temperature: f64,
    /// "Feels like" temperature
    apparent_temperature: f64,
    /// Relative humidity 0–100
    humidity: u8,
    /// WMO weather code
    weather_code: u32,
    condition: WeatherCondition,
    /// Icon name for current condition
    icon: String,
    /// Wind speed in configured units
    wind_speed: f64,
    /// Wind direction in degrees (0=N, 90=E, 180=S, 270=W)
    wind_direction: u16,
    /// Surface pressure in hPa
    pressure: f64,
    /// Observation time
    time: String,
}

/// Hourly forecast entry
struct WeatherHourly {
    time: String,
    temperature: f64,
    weather_code: u32,
    condition: WeatherCondition,
    icon: String,
    /// Precipitation probability 0–100
    precipitation_probability: u8,
}

/// Daily forecast entry
struct WeatherDaily {
    date: String,
    weather_code: u32,
    condition: WeatherCondition,
    icon: String,
    temperature_max: f64,
    temperature_min: f64,
    sunrise: String,
    sunset: String,
    uv_index_max: f64,
    /// Total precipitation in mm
    precipitation_sum: f64,
}

/// Weather location, emitted on `weather.location`
struct WeatherLocation {
    latitude: f64,
    longitude: f64,
    city: Option<String>,
    /// Whether using geolocation auto-detect or manual override
    is_manual: bool,
}
```

## Icons

WMO code to freedesktop icon mapping:

Day icons:
- 0 → `weather-clear-symbolic`
- 1 → `weather-few-clouds-symbolic`
- 2 → `weather-few-clouds-symbolic`
- 3 → `weather-overcast-symbolic`
- 45, 48 → `weather-fog-symbolic`
- 51–57 → `weather-showers-scattered-symbolic`
- 61–67 → `weather-showers-symbolic`
- 71–77 → `weather-snow-symbolic`
- 80–82 → `weather-showers-symbolic`
- 85–86 → `weather-snow-symbolic`
- 95–99 → `weather-storm-symbolic`

Night variants (use between sunset and sunrise):
- 0 → `weather-clear-night-symbolic`
- 1, 2 → `weather-few-clouds-night-symbolic`

All icons above are available in Adwaita icon theme.

## Crates

- `reqwest` — HTTP client (already in workspace)
- `serde` / `serde_json` — JSON parsing (already in workspace)

## Change Detection

**Timer-driven:** Fetch new data every 15–30 minutes. No push notifications from weather APIs.

**Location change:** Re-fetch when geolocation provider reports a new position.

## Features

- Current conditions: temperature, feels-like, humidity, wind, pressure, weather code
- Hourly forecast (next 4 hours) with precipitation probability
- 10-day daily forecast with min/max temp, sunrise/sunset, UV index, precipitation
- WMO weather code to condition and icon mapping
- Day/night icon variants based on sunrise/sunset
- Configurable units (Celsius/Fahrenheit, km/h / mph / m/s / knots)
- Auto-location from geolocation provider
- Manual location override with city name
- Configurable fetch interval and cache TTL
- Multiple saved locations (future)
- Severe weather alerts (future: Open-Meteo alerts API)
- Air quality index (future: Open-Meteo air quality API)

## Notes

- Open-Meteo is free and requires no API key — ideal for open-source projects
- Cache responses to avoid excessive API calls — respect rate limits
- Use `timezone=auto` parameter to get times in local timezone
- WMO weather codes are standard and used by most weather APIs
- Night icons should be used between sunset and sunrise times from the daily forecast
- Location comes from the geolocation provider — subscribe to `geolocation.position` for updates
