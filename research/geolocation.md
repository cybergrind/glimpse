# Geolocation Provider

**Source:** GeoClue2 D-Bus (`org.freedesktop.GeoClue2`, system bus)

**What it does:** Provides geographic location (latitude, longitude, accuracy) for use by other providers (nightlight sunrise/sunset, weather location, timezone auto-detect).

## System Interface

### org.freedesktop.GeoClue2.Manager (object: `/org/freedesktop/GeoClue2/Manager`)

Methods:
- `GetClient() -> ObjectPath` — create a new location client

### org.freedesktop.GeoClue2.Client (object: `/org/freedesktop/GeoClue2/Client/{N}`)

Properties:
- `Location: ObjectPath` (RO) — current location object path
- `DesktopId: String` (RW) — .desktop file ID of the application (required before Start)
- `RequestedAccuracyLevel: u32` (RW) — accuracy level: 0=none, 1=country, 4=city, 5=neighborhood, 6=street, 8=exact
- `Active: bool` (RO) — whether client is actively getting locations
- `DistanceThreshold: u32` (RW) — minimum distance (meters) before new location is reported
- `TimeThreshold: u32` (RW) — minimum time (seconds) between updates

Methods:
- `Start()` — begin receiving location updates
- `Stop()` — stop updates

Signals:
- `LocationUpdated(old: ObjectPath, new: ObjectPath)` — location changed

### org.freedesktop.GeoClue2.Location (object: `/org/freedesktop/GeoClue2/Location/{N}`)

Properties (all read-only):
- `Latitude: f64` — degrees North (negative = South)
- `Longitude: f64` — degrees East (negative = West)
- `Accuracy: f64` — accuracy in meters
- `Altitude: f64` — meters above sea level (may be -DBL_MAX if unknown)
- `Speed: f64` — m/s (may be -1 if unknown)
- `Heading: f64` — degrees 0–360, North=0, East=90 (may be -1 if unknown)
- `Description: String` — human-readable location description
- `Timestamp: (u64, u64)` — (seconds, microseconds) since Unix epoch

## Topics

- `geolocation.position` — current lat/lon/accuracy
- `geolocation.status` — whether location is available, accuracy level

## Methods

- `geolocation.refresh()` — request a fresh location update
- `geolocation.set_manual(latitude: f64, longitude: f64)` — override with manual location
- `geolocation.clear_manual()` — remove manual override, use GeoClue2

## Types

```rust
/// Requested accuracy level
enum AccuracyLevel {
    None,
    Country,
    City,
    Neighborhood,
    Street,
    Exact,
}

/// Current location, emitted on `geolocation.position`
struct GeoPosition {
    latitude: f64,
    longitude: f64,
    /// Accuracy in meters
    accuracy: f64,
    /// Meters above sea level (None if unknown)
    altitude: Option<f64>,
    /// Human-readable description (may be empty)
    description: String,
    /// When this position was determined
    timestamp: u64,
    /// Whether this is a manual override
    is_manual: bool,
}

/// Geolocation status, emitted on `geolocation.status`
struct GeoStatus {
    /// Whether location is available
    available: bool,
    /// Current accuracy level
    accuracy_level: AccuracyLevel,
    /// Whether using manual override
    manual_override: bool,
}
```

## Icons

- `find-location-symbolic` — location/GPS
- `mark-location-symbolic` — location pin

All icons above are available in Adwaita icon theme.

## Crates

- `zbus` (5) — D-Bus client for GeoClue2

## Change Detection

**`LocationUpdated` signal** on GeoClue2 Client. Fully reactive — fires when location changes beyond the configured threshold.

## Features

- Automatic location via GeoClue2 (WiFi-based, GPS, IP geolocation)
- Configurable accuracy level (country to exact)
- Distance and time thresholds to reduce update frequency
- Manual location override
- Altitude, speed, heading (when available)
- Human-readable location description
- Shared by nightlight (sunrise/sunset) and weather (forecast location) providers

## Notes

- GeoClue2 requires user consent on first use via a polkit agent
- `DesktopId` must be set before calling `Start()` — use the daemon's .desktop file ID
- Location accuracy depends on available hardware (WiFi scanning, GPS, IP fallback)
- On desktops without WiFi/GPS, accuracy may be city-level at best (IP geolocation)
- The daemon should request `City` accuracy by default (sufficient for sunrise/sunset and weather)
- Manual override should persist across daemon restarts (save to config)
- Active GeoClue2 client sessions feed into the privacy provider for location-in-use indicators
