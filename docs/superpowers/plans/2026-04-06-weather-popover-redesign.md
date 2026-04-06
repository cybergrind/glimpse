# Weather Popover Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the weather popover so it keeps the compact hero, shows a configurable 5-hour future strip, improves the details presentation, and restores richer forecast rows.

**Architecture:** Keep the existing weather applet data flow and typed payloads, but add independent config for hourly and daily row counts and adjust the popover rendering helpers. Most of the work stays in `popover.rs`, with small supporting updates in `config.rs`, `applet.rs`, and `research/weather.md`.

**Tech Stack:** Rust, GTK4, Relm4, serde, chrono

---

### Task 1: Add config support for a configurable hourly strip

**Files:**
- Modify: `glimpse-panel/src/applets/weather/config.rs`
- Modify: `glimpse-panel/src/applets/weather/applet.rs`
- Test: `glimpse-panel/src/applets/weather/config.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn default_weather_config_uses_five_hourly_slots() {
    let cfg = WeatherConfig::default();
    assert_eq!(cfg.hourly_slots, 5);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel default_weather_config_uses_five_hourly_slots -- --exact`
Expected: FAIL because `hourly_slots` does not exist yet.

- [ ] **Step 3: Write minimal implementation**

```rust
pub struct WeatherConfig {
    pub city_name: String,
    pub geolocate: bool,
    pub hourly_slots: usize,
    pub forecast_days: usize,
    pub label_format: String,
    pub tooltip_format: String,
    pub refresh_interval: u64,
}
```

```rust
Self {
    city_name: String::new(),
    geolocate: false,
    hourly_slots: 5,
    forecast_days: 5,
    label_format: "{temp}°".into(),
    tooltip_format: "{condition} · {temp}° · {location}".into(),
    refresh_interval: 1800,
}
```

Pass the new config field into the popover:

```rust
WeatherPopoverInit {
    parent: root.clone(),
    hourly_slots: init.hourly_slots,
    forecast_days: init.forecast_days,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p glimpse-panel default_weather_config_uses_five_hourly_slots -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/applets/weather/config.rs glimpse-panel/src/applets/weather/applet.rs
git commit -m "feat: add configurable hourly weather slots"
```

### Task 2: Render a configurable future hourly strip

**Files:**
- Modify: `glimpse-panel/src/applets/weather/applet.rs`
- Modify: `glimpse-panel/src/applets/weather/popover.rs`
- Test: `glimpse-panel/src/applets/weather/applet.rs`
- Test: `glimpse-panel/src/applets/weather/popover.rs`

- [ ] **Step 1: Write the failing tests**

```rust
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
fn visible_hourly_slots_clamps_to_zero_through_eight() {
    assert_eq!(visible_hourly_slots(0, 6), 0);
    assert_eq!(visible_hourly_slots(5, 6), 5);
    assert_eq!(visible_hourly_slots(12, 6), 6);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p glimpse-panel parse_hourly_returns_five_future_slots -- --exact`
Expected: FAIL because the parser still uses the older fixed behavior.

Run: `cargo test -p glimpse-panel visible_hourly_slots_clamps_to_zero_through_eight -- --exact`
Expected: FAIL because the limiter does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Update the parser signature:

```rust
fn parse_hourly(h: &serde_json::Value, slot_count: usize) -> Vec<WeatherHourly> {
    let mut hourly = Vec::new();
    let count = visible_hourly_slots(slot_count, times.len().saturating_sub(start + 1));

    for offset in 1..=count {
        let i = start + offset;
        ...
    }

    hourly
}
```

Add a limiter in `popover.rs`:

```rust
fn visible_hourly_slots(configured: usize, available: usize) -> usize {
    configured.min(8).min(available)
}
```

Store `hourly_slots` on the popover model and hide the hourly section when the resolved count is zero.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p glimpse-panel parse_hourly_returns_five_future_slots -- --exact`
Expected: PASS

Run: `cargo test -p glimpse-panel visible_hourly_slots_clamps_to_zero_through_eight -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/applets/weather/applet.rs glimpse-panel/src/applets/weather/popover.rs
git commit -m "feat: expand weather hourly strip"
```

### Task 3: Improve the details section hierarchy

**Files:**
- Modify: `glimpse-panel/src/applets/weather/popover.rs`
- Test: `glimpse-panel/src/applets/weather/popover.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn build_details_rows_formats_wind_and_sun_compactly() {
    let current = WeatherCurrent {
        humidity: 82,
        wind_speed: 18.0,
        wind_direction_label: "NW".into(),
        pressure: 1008.0,
        precipitation: 1.2,
        uv_index: 1.0,
        ..WeatherCurrent::default()
    };
    let today = WeatherDaily {
        temperature_min: 8.0,
        temperature_max: 14.0,
        sunrise: "2099-01-01T06:12".into(),
        sunset: "2099-01-01T19:48".into(),
        ..WeatherDaily::default()
    };

    let rows = build_details_rows(
        &current,
        Some(&today),
        Some((today.sunrise.as_str(), today.sunset.as_str())),
    );

    assert_eq!(rows[3], ("Wind".into(), "18 km/h NW".into()));
    assert_eq!(rows[7], ("Sun".into(), "06:12 / 19:48".into()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel build_details_rows_formats_wind_and_sun_compactly -- --exact`
Expected: FAIL because the current detail labels do not match.

- [ ] **Step 3: Write minimal implementation**

```rust
vec![
    ("High".into(), high),
    ("Low".into(), low),
    ("Humidity".into(), format!("{}%", current.humidity)),
    ("Wind".into(), wind),
    ("Rain".into(), format!("{:.1} mm", current.precipitation)),
    ("Pressure".into(), format!("{:.0} hPa", current.pressure)),
    ("UV index".into(), format!("{:.0}", current.uv_index)),
    (
        "Sun".into(),
        format!(
            "{} / {}",
            display_time_or_dash(sunrise),
            display_time_or_dash(sunset)
        ),
    ),
]
```

Adjust the detail pair spacing and alignment so values read as the dominant part of each pair.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p glimpse-panel build_details_rows_formats_wind_and_sun_compactly -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/applets/weather/popover.rs
git commit -m "refactor: improve weather detail emphasis"
```

### Task 4: Restore richer forecast rows

**Files:**
- Modify: `glimpse-panel/src/applets/weather/popover.rs`
- Test: `glimpse-panel/src/applets/weather/popover.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn forecast_detail_includes_precipitation_hint_when_present() {
    let rainy = WeatherDaily {
        condition: "Rain".into(),
        precipitation_sum: 3.2,
        ..WeatherDaily::default()
    };
    let dry = WeatherDaily {
        condition: "Cloudy".into(),
        precipitation_sum: 0.0,
        ..WeatherDaily::default()
    };

    assert_eq!(forecast_detail(&rainy), "Rain · 3mm");
    assert_eq!(forecast_detail(&dry), "Cloudy");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel forecast_detail_includes_precipitation_hint_when_present -- --exact`
Expected: FAIL because the helper does not exist yet.

- [ ] **Step 3: Write minimal implementation**

```rust
fn forecast_detail(entry: &WeatherDaily) -> String {
    if entry.precipitation_sum > 0.0 {
        format!("{} · {:.0}mm", entry.condition, entry.precipitation_sum)
    } else {
        entry.condition.clone()
    }
}
```

Use the helper from `build_forecast_row`, keeping temperatures aligned at the far right as `low / high`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p glimpse-panel forecast_detail_includes_precipitation_hint_when_present -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/applets/weather/popover.rs
git commit -m "refactor: restore richer weather forecast rows"
```

### Task 5: Align the note and run verification

**Files:**
- Modify: `research/weather.md`

- [ ] **Step 1: Update the research note**

```md
- `hourly_slots` controls the future strip with default `5` and range `0..=8`
- the future strip starts at `+1h`
- details use 8 paired facts with stronger value emphasis
- the forecast starts from tomorrow and keeps precipitation hints in-row
```

- [ ] **Step 2: Run targeted verification**

Run: `cargo test -p glimpse-panel weather -- --nocapture`
Expected: PASS

- [ ] **Step 3: Run final verification**

Run: `cargo test -p glimpse-panel -- --nocapture`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add research/weather.md
git commit -m "docs: refresh weather popover notes"
```
