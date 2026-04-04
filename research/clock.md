# Clock Provider

**Source:** System clock, chrono/chrono-tz

**What it does:** Provides current time and date with configurable tick intervals. Supports world clock via per-timezone topics — subscribe to any IANA timezone and receive ticks for it.

## System Interface

No external service — uses system clock directly via Rust's `std::time` and `chrono` crate.

### Time sources

- `std::time::SystemTime::now()` — current UTC time
- `chrono::Local::now()` — local time with timezone
- `chrono_tz` — convert to arbitrary timezones (complete IANA database built-in)

### NTP sync status

Available from the locale provider's `org.freedesktop.timedate1`:
- `NTPSynchronized: bool` — whether system time is synced

## Topics

- `clock.time.local` — local time (ticks at configurable interval)
- `clock.time.utc` — UTC time
- `clock.time.{iana_timezone}` — time in a specific timezone (e.g. `clock.time.America/New_York`, `clock.time.Europe/Tokyo`)
- `clock.date` — current date in local timezone (ticks at midnight)

### World clock

Each timezone is a separate topic. A panel showing a world clock subscribes to:
```
clock.time.local
clock.time.America/New_York
clock.time.Asia/Tokyo
clock.time.Europe/London
```

The provider only ticks timezones that have at least one subscriber. When the last subscriber for a timezone unsubscribes, that timezone stops ticking.

Subscribing to `clock.time.**` receives ticks for all currently active timezones.

## Methods

- `clock.set_tick_interval(milliseconds: u32)` — configure tick frequency (default 1000ms, range 100–60000)
- `clock.list_timezones() -> Vec<TimezoneInfo>` — list all available IANA timezones with current UTC offsets

## Types

```rust
/// Time in a timezone, emitted on `clock.time.{timezone}` at each tick
struct ClockTime {
    /// IANA timezone identifier (e.g. "America/New_York") or "local" or "utc"
    timezone: String,
    /// Timezone abbreviation (e.g. "EST", "CET", "JST")
    abbr: String,
    /// Unix timestamp (seconds since epoch)
    timestamp: u64,
    hour: u8,
    minute: u8,
    second: u8,
    /// UTC offset in seconds (e.g. -18000 for EST, 3600 for CET)
    utc_offset: i32,
}

/// Current date in local timezone, emitted on `clock.date` at midnight
struct ClockDate {
    year: i32,
    /// 1–12
    month: u8,
    /// 1–31
    day: u8,
    /// 0=Monday, 6=Sunday (ISO 8601)
    day_of_week: u8,
    /// 1–366
    day_of_year: u16,
    /// ISO week number (1–53)
    week_number: u8,
}

/// A timezone entry, returned by `clock.list_timezones`
struct TimezoneInfo {
    /// IANA identifier (e.g. "America/New_York")
    id: String,
    /// Current abbreviation (e.g. "EST" or "EDT" depending on DST)
    abbr: String,
    /// Current UTC offset in seconds
    utc_offset: i32,
}
```

## Icons

- `preferences-system-time-symbolic` — clock/time

## Crates

- `chrono` — date/time handling (already in workspace)
- `chrono-tz` — IANA timezone database (already in workspace)

## Change Detection

**Timer-driven:** The provider runs an internal timer at the configured tick interval (default 1 second). On each tick, emits `ClockTime` for every timezone that has subscribers. Emits `clock.date` once at midnight (or on startup if date has changed since last run).

No external change detection — the provider is the source of truth.

## Features

- Local time with configurable tick interval (100ms to 60s)
- UTC time
- World clock: subscribe to any IANA timezone by topic
- Per-timezone ticking — only active timezones consume CPU
- Timezone abbreviation with DST awareness (EST vs EDT)
- UTC offset per timezone
- Date with day-of-week, week number, day-of-year
- Midnight date-change event
- List all available timezones with current offsets
- Unix timestamp for machine use
- NTP sync status (from locale provider)

## Notes

- `chrono-tz` provides the complete IANA timezone database — no external files needed
- Default 1-second tick is sufficient for most panel clocks
- Date change event is useful for calendar widgets to refresh
- Timezone topics use IANA identifiers with `/` in them (e.g. `clock.time.America/New_York`) — the `/` is part of the topic segment, not a separator. Topic segments are dot-separated, so `America/New_York` is one segment.
- Consider coalescing: if no client is subscribed to any `clock.time.*` topic, don't run the timer at all
- DST transitions change the abbreviation and offset — the provider emits the correct values automatically
