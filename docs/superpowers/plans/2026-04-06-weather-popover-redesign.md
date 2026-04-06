# Weather Popover Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the weather popover so it feels calmer and more spacious while preserving the hero, 4-hour forecast, details, and a configurable forecast section.

**Architecture:** Keep the existing weather applet data flow and typed message payloads, but reorganize the popover widget tree and update the formatting logic to match the approved hierarchy. The redesign stays local to the weather applet, centered on `popover.rs`, with minimal supporting changes in `applet.rs` and `config.rs` for derived values and configurable forecast row count.

**Tech Stack:** Rust, GTK4, Relm4, serde, chrono

---

### Task 1: Reshape the hero section to the approved two-row layout

**Files:**
- Modify: `glimpse-panel/src/applets/weather/popover.rs`
- Test: `glimpse-panel/src/applets/weather/popover.rs`

- [ ] **Step 1: Write the failing test**

Add a focused helper test that proves the hero metadata line contains condition and feels-like, but not high/low:

```rust
#[test]
fn hero_summary_formats_condition_and_feels_like_only() {
    let current = WeatherCurrent {
        condition: "Overcast".into(),
        apparent_temperature: 9.0,
        ..WeatherCurrent::default()
    };

    let summary = hero_summary(&current);

    assert_eq!(summary, "Overcast · Feels like 9°");
    assert!(!summary.contains("High"));
    assert!(!summary.contains("Low"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel hero_summary_formats_condition_and_feels_like_only -- --exact`
Expected: FAIL because `hero_summary` does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Add a helper and refactor the hero widgets:

```rust
fn hero_summary(current: &WeatherCurrent) -> String {
    format!("{} · Feels like {:.0}°", current.condition, current.apparent_temperature)
}
```

Update the hero structure in `init()` so it becomes:

```rust
let hero_row1 = gtk::Box::new(gtk::Orientation::Horizontal, 8);
let hero_meta = gtk::Label::new(None);
let hero_location = gtk::Label::new(None);

hero_meta.add_css_class("weather-hero-meta");
hero_location.add_css_class("weather-hero-location");
hero_location.set_halign(gtk::Align::End);
hero_location.set_hexpand(true);
```

Store `hero_location` instead of `hero_hilo`, and in `UpdateCurrent` set:

```rust
self.hero_icon.set_icon_name(Some(icon));
self.hero_temp.set_label(&format!("{temp:.0}°"));
self.hero_condition.set_label(&hero_summary(&data));
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p glimpse-panel hero_summary_formats_condition_and_feels_like_only -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/applets/weather/popover.rs
git commit -m "refactor: simplify weather hero layout"
```

### Task 2: Make the 4-hour forecast start at +1 hour and render exactly four slots

**Files:**
- Modify: `glimpse-panel/src/applets/weather/applet.rs`
- Modify: `glimpse-panel/src/applets/weather/popover.rs`
- Test: `glimpse-panel/src/applets/weather/applet.rs`

- [ ] **Step 1: Write the failing test**

Add a parser test for hourly selection:

```rust
#[test]
fn parse_hourly_returns_four_future_slots() {
    let data = serde_json::json!({
        "time": [
            "2099-01-01T10:00",
            "2099-01-01T11:00",
            "2099-01-01T12:00",
            "2099-01-01T13:00",
            "2099-01-01T14:00",
            "2099-01-01T15:00"
        ],
        "temperature_2m": [10.0, 11.0, 12.0, 13.0, 14.0, 15.0],
        "weather_code": [0, 1, 2, 3, 61, 63],
        "is_day": [1, 1, 1, 1, 1, 1]
    });

    let hourly = parse_hourly(&data);

    assert_eq!(hourly.len(), 4);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel parse_hourly_returns_four_future_slots -- --exact`
Expected: FAIL because the parser currently produces five slots.

- [ ] **Step 3: Write minimal implementation**

In `parse_hourly`, replace the offsets list:

```rust
for &offset in &[1, 2, 3, 4] {
```

In `WeatherPopoverInput::UpdateHourly`, render the full vector without `.take(5)`:

```rust
for entry in &data {
    self.hourly_box.append(&build_hourly_col(entry));
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p glimpse-panel parse_hourly_returns_four_future_slots -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/applets/weather/applet.rs glimpse-panel/src/applets/weather/popover.rs
git commit -m "refactor: show only four future weather slots"
```

### Task 3: Replace stat tiles with a calm 8-item details grid

**Files:**
- Modify: `glimpse-panel/src/applets/weather/popover.rs`
- Test: `glimpse-panel/src/applets/weather/popover.rs`

- [ ] **Step 1: Write the failing test**

Add a pure formatting test for the details model:

```rust
#[test]
fn build_details_rows_returns_eight_items() {
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
        ..WeatherDaily::default()
    };

    let rows = build_details_rows(&current, Some(&today), None);

    assert_eq!(rows.len(), 8);
    assert_eq!(rows[0], ("High".into(), "14°".into()));
    assert_eq!(rows[1], ("Low".into(), "8°".into()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel build_details_rows_returns_eight_items -- --exact`
Expected: FAIL because `build_details_rows` does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Add a helper that returns the approved eight items:

```rust
fn build_details_rows(
    current: &WeatherCurrent,
    today: Option<&WeatherDaily>,
    sun: Option<(&str, &str)>,
) -> Vec<(String, String)> {
    let (high, low) = today
        .map(|day| (format!("{:.0}°", day.temperature_max), format!("{:.0}°", day.temperature_min)))
        .unwrap_or_else(|| ("—".into(), "—".into()));
    let (sunrise, sunset) = sun.unwrap_or(("—", "—"));

    vec![
        ("High".into(), high),
        ("Low".into(), low),
        ("Humidity".into(), format!("{}%", current.humidity)),
        ("Wind".into(), format!("{:.0} km/h {}", current.wind_speed, current.wind_direction_label)),
        ("Rain".into(), format!("{:.1} mm", current.precipitation)),
        ("Pressure".into(), format!("{:.0} hPa", current.pressure)),
        ("UV index".into(), format!("{:.0}", current.uv_index)),
        ("Sunrise/Sunset".into(), format!("{sunrise} / {sunset}")),
    ]
}
```

Refactor `UpdateCurrent` and `UpdateForecast` to rebuild `stats_box` using row pairs instead of `build_stat_tile`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p glimpse-panel build_details_rows_returns_eight_items -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/applets/weather/popover.rs
git commit -m "refactor: rebuild weather details as balanced rows"
```

### Task 4: Make the forecast section configurable and visually lighter

**Files:**
- Modify: `glimpse-panel/src/applets/weather/config.rs`
- Modify: `glimpse-panel/src/applets/weather/applet.rs`
- Modify: `glimpse-panel/src/applets/weather/popover.rs`
- Test: `glimpse-panel/src/applets/weather/config.rs`
- Test: `glimpse-panel/src/applets/weather/popover.rs`

- [ ] **Step 1: Write the failing test**

Add a config default test and a forecast limiter test:

```rust
#[test]
fn default_weather_config_uses_five_forecast_days() {
    let cfg = WeatherConfig::default();
    assert_eq!(cfg.forecast_days, 5);
}

#[test]
fn visible_forecast_rows_clamps_to_zero_through_ten() {
    assert_eq!(visible_forecast_rows(0, 8), 0);
    assert_eq!(visible_forecast_rows(5, 8), 5);
    assert_eq!(visible_forecast_rows(12, 8), 8);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p glimpse-panel default_weather_config_uses_five_forecast_days -- --exact`
Expected: FAIL because `forecast_days` does not exist yet.

Run: `cargo test -p glimpse-panel visible_forecast_rows_clamps_to_zero_through_ten -- --exact`
Expected: FAIL because `visible_forecast_rows` does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Add the config field:

```rust
pub struct WeatherConfig {
    pub city_name: String,
    pub geolocate: bool,
    pub forecast_days: usize,
    ...
}
```

Default:

```rust
forecast_days: 5,
```

Add a limiter in `popover.rs`:

```rust
fn visible_forecast_rows(configured: usize, available: usize) -> usize {
    configured.min(10).min(available)
}
```

Store `forecast_days` on `WeatherPopoverInit` and `WeatherPopover`, then:

```rust
let count = visible_forecast_rows(self.forecast_days, data.len());
if count == 0 {
    return;
}
for entry in data.iter().take(count) {
    self.forecast_box.append(&build_forecast_row(entry));
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p glimpse-panel default_weather_config_uses_five_forecast_days -- --exact`
Expected: PASS

Run: `cargo test -p glimpse-panel visible_forecast_rows_clamps_to_zero_through_ten -- --exact`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/applets/weather/config.rs glimpse-panel/src/applets/weather/applet.rs glimpse-panel/src/applets/weather/popover.rs
git commit -m "feat: make weather forecast row count configurable"
```

### Task 5: Run verification and align documentation

**Files:**
- Modify: `research/weather.md`

- [ ] **Step 1: Write the failing doc expectation**

Record the mismatch:

```text
The current research note does not describe the approved calm popover hierarchy with compact hero, four future slots, 8-item details, and optional 10-day outlook.
```

- [ ] **Step 2: Run targeted verification before docs**

Run: `cargo test -p glimpse-panel weather -- --nocapture`
Expected: PASS

- [ ] **Step 3: Write minimal documentation update**

Update `research/weather.md` to describe:

```md
- compact two-row hero
- 4-hour strip starts at +1 hour
- details section uses 8 key/value items
- forecast section defaults to 5 rows and supports `0..=10`
```

- [ ] **Step 4: Run final verification**

Run: `cargo test -p glimpse-panel -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add research/weather.md
git commit -m "docs: update weather popover layout notes"
```
