# Battery Provider

**Source:** UPower D-Bus (`org.freedesktop.UPower`, system bus)

**What it does:** Monitors battery and power supply devices — charge level, charging state, health, time estimates, and peripheral device batteries (mice, keyboards, headsets).

## System Interface

**D-Bus service:** `org.freedesktop.UPower`

### org.freedesktop.UPower (object: `/org/freedesktop/UPower`)

Methods:
- `EnumerateDevices() -> Vec<ObjectPath>` — list all power devices
- `GetDisplayDevice() -> ObjectPath` — composite battery for multi-battery systems
- `EnumerateKbdBacklights() -> Vec<ObjectPath>` — list keyboard backlight LEDs
- `GetCriticalAction() -> String` — action on critical battery: HybridSleep, Hibernate, PowerOff, Suspend, Ignore

Properties:
- `DaemonVersion: String` — running daemon version
- `OnBattery: bool` — whether system is running on battery
- `LidIsClosed: bool`
- `LidIsPresent: bool`

Signals:
- `DeviceAdded(ObjectPath)`
- `DeviceRemoved(ObjectPath)`

### org.freedesktop.UPower.Device (object: `/org/freedesktop/UPower/devices/{name}`)

Methods:
- `Refresh()` — force data refresh from hardware
- `GetHistory(type: String, timespan: u32, resolution: u32) -> Vec<(u32, f64, u32)>` — historical data ("rate", "charge", "voltage")
- `GetStatistics(type: String) -> Vec<(f64, f64)>` — session statistics ("charging", "discharging")
- `EnableChargeThreshold(enabled: bool)` — activate/deactivate charge limiting

Properties — metadata:
- `NativePath: String` — OS-specific path (sysfs on Linux)
- `Vendor: String`
- `Model: String`
- `Serial: String`
- `UpdateTime: u64` — timestamp of last data read
- `IconName: String` — icon per Icon Naming Specification

Properties — classification:
- `Type: u32` — 0=Unknown, 1=LinePower, 2=Battery, 3=UPS, 4=Monitor, 5=Mouse, 6=Keyboard, 7=PDA, 8=Phone, 9=MediaPlayer, 10=Tablet, 11=Headset, 12=Headphones
- `PowerSupply: bool` — whether device supplies system power

Properties — status:
- `HasHistory: bool`
- `HasStatistics: bool`
- `IsPresent: bool` — battery physically present in bay
- `IsRechargeable: bool`
- `Online: bool` — receiving line power (line power devices only)
- `State: u32` — 0=Unknown, 1=Charging, 2=Discharging, 3=Empty, 4=FullyCharged, 5=PendingCharge, 6=PendingDischarge
- `WarningLevel: u32` — 0=Unknown, 1=None, 2=Low, 3=Critical, 4=Action
- `BatteryLevel: u32` — coarse level: 0=Unknown, 1=None, 3=Low, 4=Critical, 6=Normal, 7=High, 8=Full

Properties — energy:
- `Energy: f64` — current energy in Wh
- `EnergyEmpty: f64` — energy at empty in Wh
- `EnergyFull: f64` — energy at full in Wh
- `EnergyFullDesign: f64` — factory design capacity in Wh
- `EnergyRate: f64` — drain/charge rate in W
- `Voltage: f64` — current voltage in V
- `VoltageMinDesign: f64` — minimum design voltage in V
- `VoltageMaxDesign: f64` — maximum design voltage in V

Properties — charge/capacity:
- `Percentage: f64` — 0.0–100.0
- `Capacity: f64` — 0.0–100.0, battery health (age-related)
- `Temperature: f64` — degrees Celsius
- `ChargeCycles: i32` — complete charge cycles, -1 if unknown
- `ChargeStartThreshold: u32` — charge resumption threshold (0–100, or u32::MAX to skip)
- `ChargeEndThreshold: u32` — charge halt threshold (0–100, or u32::MAX to skip)
- `ChargeThresholdEnabled: bool` — whether charge limits are active
- `ChargeThresholdSupported: bool` — whether hardware supports charge limits

Properties — time:
- `TimeToEmpty: i64` — seconds until empty (0 if unknown)
- `TimeToFull: i64` — seconds until full (0 if unknown)

Properties — technology:
- `Technology: u32` — 0=Unknown, 1=LithiumIon, 2=LithiumPolymer, 3=LithiumIronPhosphate, 4=LeadAcid, 5=NickelCadmium, 6=NickelMetalHydride

Signals:
- `PropertiesChanged` (via `org.freedesktop.DBus.Properties`)

## Topics

- `battery.status` — primary battery snapshot
- `battery.devices` — all UPower devices with battery info

## Methods

None (read-only provider)

## Types

```rust
/// Charging state of a battery device
enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    Empty,
    FullyCharged,
    PendingCharge,
    PendingDischarge,
}

/// How urgently the battery needs attention
enum WarningLevel {
    Unknown,
    None,
    Low,
    Critical,
    Action,
}

/// Coarse battery level for devices without fine-grained reporting
enum BatteryLevel {
    Unknown,
    None,
    Low,
    Critical,
    Normal,
    High,
    Full,
}

/// UPower device classification
enum DeviceType {
    Unknown,
    LinePower,
    Battery,
    UPS,
    Monitor,
    Mouse,
    Keyboard,
    PDA,
    Phone,
    MediaPlayer,
    Tablet,
    Headset,
    Headphones,
}

/// Battery chemistry
enum Technology {
    Unknown,
    LithiumIon,
    LithiumPolymer,
    LithiumIronPhosphate,
    LeadAcid,
    NickelCadmium,
    NickelMetalHydride,
}

/// Primary battery state, emitted on `battery.status`
struct BatteryStatus {
    /// Charge percentage 0–100
    percentage: u8,
    state: BatteryState,
    icon_name: String,
    time_to_empty: Option<Duration>,
    time_to_full: Option<Duration>,
    /// Discharge/charge rate in watts
    energy_rate: f64,
    voltage: f64,
    temperature: Option<f64>,
    /// Battery health 0.0–100.0
    capacity: f64,
    warning_level: WarningLevel,
    technology: Technology,
    /// Complete charge cycles, None if unknown
    charge_cycles: Option<u32>,
    /// Whether system is on battery power
    on_battery: bool,
}

/// A single UPower device, used in `battery.devices` list
struct BatteryDevice {
    /// UPower object path
    id: String,
    device_type: DeviceType,
    vendor: String,
    model: String,
    serial: String,
    percentage: Option<f64>,
    state: BatteryState,
    battery_level: BatteryLevel,
    icon_name: String,
    is_present: bool,
    is_rechargeable: bool,
    /// Whether this device supplies system power
    power_supply: bool,
}
```

## Icons

Battery level (modern, 10% increments — used by UPower `IconName` property):
- `battery-level-0-symbolic` through `battery-level-100-symbolic`
- `battery-level-0-charging-symbolic` through `battery-level-100-charging-symbolic`

Battery level (legacy, still returned by some devices):
- `battery-empty-symbolic`
- `battery-caution-symbolic` / `battery-caution-charging-symbolic`
- `battery-low-symbolic` / `battery-low-charging-symbolic`
- `battery-good-symbolic` / `battery-good-charging-symbolic`
- `battery-full-symbolic` / `battery-full-charging-symbolic`
- `battery-full-charged-symbolic`

Special:
- `battery-missing-symbolic` — battery not detected
- `ac-adapter-symbolic` — line power / AC adapter

Peripheral device type icons:
- `input-mouse-symbolic` — mouse
- `input-keyboard-symbolic` — keyboard
- `input-tablet-symbolic` — tablet/drawing pad
- `input-touchpad-symbolic` — touchpad
- `input-gaming-symbolic` — game controller
- `input-dialpad-symbolic` — dialpad
- `phone-symbolic` — phone
- `audio-headset-symbolic` — headset
- `audio-headphones-symbolic` — headphones

All icons above are available in Adwaita and Papirus icon themes.

## Features

- Primary battery monitoring: percentage, state, icon, time estimates
- Energy rate (wattage), voltage, temperature
- Capacity / health percentage
- Warning level detection (low, critical, action)
- Battery technology/chemistry identification
- Charge cycle count tracking
- Peripheral device batteries (mice, keyboards, headsets via UPower)
- Combined display device for multi-battery laptops (`GetDisplayDevice()`)
- Coarse battery level for devices without fine-grained reporting
- Charge threshold control (ThinkPad/ASUS charge limits via `EnableChargeThreshold`)
- Design capacity vs current capacity (degradation %)
- Historical data: charge/discharge rate/voltage over time (`GetHistory`)
- Session statistics (`GetStatistics`)
- Keyboard backlight enumeration
- Graceful handling of no-battery systems (desktops)

## Crates

- `zbus` (5) — D-Bus client for UPower
- `upower_dbus` (0.3) — UPower-specific D-Bus bindings (optional, can use raw zbus)

## Change Detection

**Device properties:** `PropertiesChanged` D-Bus signal on each `org.freedesktop.UPower.Device` object. Fully reactive — fires on any property change (percentage, state, energy rate, etc.).

**Device add/remove:** `DeviceAdded` / `DeviceRemoved` signals on the main `org.freedesktop.UPower` object. Fires when devices are plugged/unplugged (e.g. USB peripherals).

**System power state:** `OnBattery` property change on `org.freedesktop.UPower`. Fires when switching between AC and battery power.

## Notes

- Peripheral batteries overlap with bluetooth provider data — consider cross-referencing
- `GetDisplayDevice()` returns the "combined" battery for multi-battery laptops
- Charge threshold support depends on hardware and kernel driver
