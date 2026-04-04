# Locale Provider

**Source:** System locale files, timedatectl D-Bus (`org.freedesktop.timedate1`, system bus)

**What it does:** Reports current system locale and timezone, provides timezone switching.

## System Interface

### org.freedesktop.timedate1 (object: `/org/freedesktop/timedate1`)

Methods:
- `SetTimezone(timezone: String, interactive: bool)` — set system timezone (e.g. "Europe/Warsaw")
- `SetLocalRTC(local_rtc: bool, fix_system: bool, interactive: bool)` — use local time for hardware clock
- `SetNTP(use_ntp: bool, interactive: bool)` — enable/disable NTP sync
- `SetTime(usec_utc: i64, relative: bool, interactive: bool)` — set system time

Properties:
- `Timezone: String` (RO) — current timezone (e.g. "Europe/Warsaw")
- `LocalRTC: bool` (RO) — hardware clock in local time
- `NTP: bool` (RO) — NTP synchronization enabled
- `NTPSynchronized: bool` (RO) — currently synchronized with NTP
- `TimeUSec: u64` (RO) — current system time in microseconds
- `RTCTimeUSec: u64` (RO) — hardware clock time in microseconds

Signals:
- `PropertiesChanged` (standard)

### System locale

Read from:
- `/etc/locale.conf` — `LANG=en_US.UTF-8`, `LC_*` variables
- Environment: `$LANG`, `$LC_ALL`, `$LC_MESSAGES`, etc.
- `localectl status` — shows system locale and keymap

### Timezone database

- Timezone list: `/usr/share/zoneinfo/` directory tree
- `timedatectl list-timezones` — all available timezones

## Topics

- `locale.current` — current locale settings
- `locale.timezone` — current timezone and NTP state

## Methods

- `locale.set_timezone(timezone: String)` — set system timezone
- `locale.set_ntp(enabled: bool)` — enable/disable NTP

## Types

```rust
/// Current locale settings, emitted on `locale.current`
struct LocaleInfo {
    /// Primary locale (e.g. "en_US.UTF-8")
    lang: String,
    /// Language for messages (if different from lang)
    lc_messages: Option<String>,
}

/// Timezone and time sync state, emitted on `locale.timezone`
struct TimezoneInfo {
    /// Timezone name (e.g. "Europe/Warsaw")
    timezone: String,
    /// NTP synchronization enabled
    ntp_enabled: bool,
    /// Currently synchronized
    ntp_synchronized: bool,
    /// Hardware clock in local time
    local_rtc: bool,
}
```

## Icons

- `preferences-system-time-symbolic` — time/timezone settings

## Crates

- `zbus` (5) — D-Bus client for timedate1
- `chrono-tz` — timezone database and conversions (already in workspace)

## Change Detection

**timedate1:** `PropertiesChanged` signal on `org.freedesktop.timedate1`. Fires on timezone or NTP changes. Fully reactive.

**Locale:** Locale changes require system restart or re-login — extremely infrequent. Read once at startup.

## Features

- Report current timezone
- Report NTP sync status
- Set timezone
- Enable/disable NTP
- Report current locale (LANG, LC_MESSAGES)
- Timezone list for selection UI
- Timezone search by name/city

## Notes

- Timezone changes via `SetTimezone` require polkit authentication
- Locale changes are rare and usually require session restart — not worth live-monitoring
- Timezone list can be built from `/usr/share/zoneinfo/` or `chrono-tz` crate's built-in database
