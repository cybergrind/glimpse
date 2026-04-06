# Weather Applet

**Source:** Open-Meteo forecast API, Open-Meteo geocoding API, optional IP geolocation via `ipapi.co`

**What it does:** The weather applet fetches current weather, hourly forecast, and daily forecast directly from HTTP APIs. It no longer depends on `glimpsed`.

## Ownership

- `glimpse-panel/src/applets/weather/` owns all weather logic
- `glimpsed` has no weather provider, topics, or methods

## Location Resolution

Priority:
1. `city_name` from panel config
2. Optional IP-based fallback when `use_ip_location_when_city_unset = true`
3. Otherwise do nothing

Flow:
1. Resolve a city string
2. Geocode the city with Open-Meteo
3. Fetch forecast data with the resulting coordinates
4. Refresh on the configured interval

## Geocoding

Base URL: `https://geocoding-api.open-meteo.com/v1/search`

Request:
```text
GET /v1/search?name=Warsaw,+PL&count=1&language=en&format=json
```

Expected fields from the first result:
- `name`
- `country_code`
- `latitude`
- `longitude`

The applet normalizes the label to `City, CC` when a country code is available.

## Weather API

Base URL: `https://api.open-meteo.com/v1/forecast`

Request:
```text
GET /v1/forecast?latitude=52.2298&longitude=21.0118
  &current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,
           wind_speed_10m,wind_direction_10m,surface_pressure,uv_index,is_day,precipitation
  &hourly=temperature_2m,weather_code,is_day
  &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_sum
  &forecast_days=10
  &timezone=auto
```

## Config

```toml
[applets.weather]
extends = "weather"
city_name = "Warsaw, PL"
use_ip_location_when_city_unset = false
label_format = "{temp}°"
tooltip_format = "{condition} · {temp}°"
refresh_interval = 1800
```

## File Structure

```text
glimpse-panel/src/applets/weather/
  applet.rs   — config resolution, geocoding, forecast fetch, parsing, refresh loop
  popover.rs  — hero + hourly + stats + daily
  config.rs   — applet settings
  mod.rs
```

## Notes

- Open-Meteo is free and requires no API key
- `timezone=auto` keeps hourly/daily output aligned with the resolved location
- IP geolocation is optional and intentionally disabled by default
- The applet preserves the previous UI structure by translating HTTP responses into the same internal weather shape the popover expects
