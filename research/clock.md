# Clock Provider

**Source:** System clock, chrono/chrono-tz

**What it does:** Provides current time and date with configurable tick intervals, multiple timezone support, and formatted time strings.

## System Interface

No external service — uses system clock directly via Rust's `std::time` and `chrono` crate.

### Time sources

- `std::time::SystemTime::now()` — current UTC time
- `chrono::Local::now()` — local time with timezone
- `chrono_tz` — convert to arbitrary timezones

### NTP sync status

Available from the locale provider's `org.freedesktop.timedate1`:
- `NTPSynchronized: bool` — whether system time is synced

## Topics

- `clock.time` — current time (ticks at configurable interval)
- `clock.date` — current date (ticks at midnight)

## Methods

- `clock.set_tick_interval(milliseconds: u32)` — configure how often time updates are emitted (default 1000ms)
- `clock.add_timezone(timezone: String)` — add a timezone to track
- `clock.remove_timezone(timezone: String)` — remove a tracked timezone

## Types

```rust
/// Current time state, emitted on `clock.time` at each tick
struct ClockTime {
    /// Unix timestamp (seconds)
    timestamp: u64,
    /// Local time components
    hour: u8,
    minute: u8,
    second: u8,
    /// Local timezone name (e.g. "CET", "CEST")
    timezone_abbr: String,
    /// UTC offset in seconds
    utc_offset: i32,
    /// Additional tracked timezones
    other_timezones: Vec<TimezoneTime>,
}

/// Time in a specific timezone
struct TimezoneTime {
    /// Timezone identifier (e.g. "America/New_York")
    timezone: String,
    /// Timezone abbreviation (e.g. "EST")
    abbr: String,
    hour: u8,
    minute: u8,
    second: u8,
    /// UTC offset in seconds
    utc_offset: i32,
}

/// Current date state, emitted on `clock.date` (once per day at midnight)
struct ClockDate {
    year: i32,
    /// 1-12
    month: u8,
    /// 1-31
    day: u8,
    /// 0=Monday, 6=Sunday (ISO)
    day_of_week: u8,
    /// 1-366
    day_of_year: u16,
    /// ISO week number
    week_number: u8,
}
```

## Icons

- `preferences-system-time-symbolic` — clock/time

## Crates

- `chrono` — date/time handling (already in workspace)
- `chrono-tz` — timezone database (already in workspace)

## Change Detection

**Timer-driven:** The provider runs an internal timer at the configured tick interval (default 1 second). Emits `clock.time` on each tick. Emits `clock.date` once at midnight (or on startup if date has changed).

No external change detection — the provider is the source of truth.

## Features

- Current time with configurable tick interval (100ms to 60s)
- Date with day-of-week, week number, day-of-year
- Multiple timezone support (add/remove tracked timezones)
- Timezone abbreviation and UTC offset
- NTP sync status (from locale provider)
- Formatted time strings for display
- Unix timestamp for machine use
- Midnight date-change event

## Notes

- Tick interval should be configurable per-client (panel may want 1s, a clock widget may want 100ms)
- Default 1-second tick is sufficient for most panel clocks
- `chrono-tz` provides the complete IANA timezone database — no external files needed
- Date change event is useful for calendar widgets to refresh
- Consider coalescing: if no client is subscribed to `clock.time`, don't run the timer
