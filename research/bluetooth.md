# Bluetooth Provider

**Source:** BlueZ D-Bus (`org.bluez`, system bus)

**What it does:** Lists Bluetooth adapters and devices, manages pairing/trust/connect/disconnect, controls discovery, reports device battery levels, and handles adapter power state.

## System Interface

BlueZ uses `org.freedesktop.DBus.ObjectManager` at `/org/bluez` for the entire object tree.

### ObjectManager (object: `/org/bluez`)

Methods:
- `GetManagedObjects() -> HashMap<ObjectPath, HashMap<String, HashMap<String, Variant>>>` — returns all objects with all interfaces and properties in one call

Signals:
- `InterfacesAdded(path: ObjectPath, interfaces: HashMap<String, HashMap<String, Variant>>)` — new device discovered or interface added
- `InterfacesRemoved(path: ObjectPath, interfaces: Vec<String>)` — device removed or interface lost

### org.bluez.Adapter1 (object: `/org/bluez/hci{N}`)

Methods:
- `StartDiscovery()` — begin scanning for devices
- `StopDiscovery()` — stop scanning
- `RemoveDevice(device: ObjectPath)` — remove device and pairing data
- `SetDiscoveryFilter(filter: HashMap<String, Variant>)` — filter by UUIDs, RSSI threshold, transport type
- `GetDiscoveryFilters() -> Vec<String>` — available filter keys

Properties:
- `Address: String` (RO) — adapter MAC address
- `AddressType: String` (RO) — "public" or "random"
- `Name: String` (RO) — system hostname
- `Alias: String` (RW) — customizable friendly name
- `Class: u32` (RO) — device class bits
- `Powered: bool` (RW) — adapter on/off
- `Discoverable: bool` (RW) — visible to other devices
- `DiscoverableTimeout: u32` (RW) — seconds
- `Pairable: bool` (RW) — accepts pairing requests
- `PairableTimeout: u32` (RW) — seconds
- `Discovering: bool` (RO) — currently scanning
- `UUIDs: Vec<String>` (RO) — available local services
- `Modalias: String` (RO, optional) — kernel device ID

### org.bluez.Device1 (object: `/org/bluez/hci{N}/dev_XX_XX_XX_XX_XX_XX`)

Methods:
- `Connect()` — connect all auto-connectable profiles
- `ConnectProfile(uuid: String)` — connect specific profile
- `Disconnect()` — disconnect all profiles
- `DisconnectProfile(uuid: String)` — disconnect specific profile
- `Pair()` — initiate pairing
- `CancelPairing()` — cancel ongoing pairing

Properties:
- `Address: String` (RO) — device MAC
- `AddressType: String` (RO) — "public" or "random"
- `Name: String` (RO) — remote device name
- `Alias: String` (RW) — user-set alias
- `Class: u32` (RO) — device class bits
- `Appearance: u16` (RO) — GATT appearance code
- `Icon: String` (RO) — suggested freedesktop icon name
- `Paired: bool` (RO)
- `Bonded: bool` (RO) — persistent pairing
- `Trusted: bool` (RW) — authorized for auto-connect
- `Blocked: bool` (RW)
- `Connected: bool` (RO)
- `LegacyPairing: bool` (RO) — pre-2.1 pairing
- `RSSI: i16` (RO) — signal strength in dBm
- `TxPower: i16` (RO) — transmission power in dBm
- `UUIDs: Vec<String>` (RO) — advertised service UUIDs
- `Adapter: ObjectPath` (RO) — parent adapter
- `ServicesResolved: bool` (RO) — GATT discovery complete
- `ManufacturerData: HashMap<u16, Vec<u8>>` (RO, optional) — manufacturer advertisement data
- `ServiceData: HashMap<String, Vec<u8>>` (RO, optional) — service advertisement data

### org.bluez.Battery1 (same device object, additional interface)

Properties:
- `Percentage: u8` (RO) — battery level 0–100
- `Source: String` (RO, optional) — data source ("HFP 1.7", "HID", UUID)

Note: Only present on devices that report battery (headsets, mice, keyboards via HID/HFP/BLE).

### org.bluez.AgentManager1 (object: `/org/bluez`)

Methods:
- `RegisterAgent(agent: ObjectPath, capability: String)` — register pairing agent; capability is one of: "DisplayOnly", "DisplayYesNo", "KeyboardOnly", "NoInputNoOutput", "KeyboardDisplay"
- `UnregisterAgent(agent: ObjectPath)`
- `RequestDefaultAgent(agent: ObjectPath)` — make this agent handle all pairing

### org.bluez.Agent1 (implemented by the daemon)

Methods BlueZ calls on the agent during pairing:
- `RequestPinCode(device: ObjectPath) -> String` — legacy PIN (1-16 chars)
- `RequestPasskey(device: ObjectPath) -> u32` — numeric passkey (0-999999)
- `DisplayPinCode(device: ObjectPath, pincode: String)` — show PIN
- `DisplayPasskey(device: ObjectPath, passkey: u32, entered: u16)` — show passkey with typing progress
- `RequestConfirmation(device: ObjectPath, passkey: u32)` — confirm 6-digit passkey
- `RequestAuthorization(device: ObjectPath)` — authorize incoming pairing
- `AuthorizeService(device: ObjectPath, uuid: String)` — grant service access
- `Cancel()` — pairing cancelled
- `Release()` — agent unregistered

### Device class decoding

Major device class from `Class` property: `(class >> 8) & 0x1F`

- `0x01` = Computer (desktop, laptop, server, tablet)
- `0x02` = Phone (cellular, cordless, smartphone)
- `0x03` = LAN/Network Access Point
- `0x04` = Audio/Video (headset, speaker, microphone, camcorder)
- `0x05` = Peripheral (keyboard, mouse, joystick, gamepad)
- `0x06` = Imaging (printer, scanner, camera)
- `0x07` = Wearable (watch, pager, jacket, helmet)
- `0x08` = Toy
- `0x09` = Health (blood pressure, thermometer, heart rate)
- `0x1F` = Uncategorized

### Connection workflows

Pair a new device:
1. `Adapter.StartDiscovery()`
2. Wait for `InterfacesAdded` with `org.bluez.Device1`
3. `Device.Pair()` — agent handles user interaction
4. `Device.Trusted = true` — allow auto-connect
5. `Device.Connect()` — connect profiles
6. `Adapter.StopDiscovery()`

Connect to paired device:
1. `Device.Connect()` — connects all auto-connectable profiles
2. Monitor `Device.Connected` via `PropertiesChanged`

Forget/remove device:
1. `Adapter.RemoveDevice(device_path)` — removes pairing and bond data

## Topics

- `bluetooth.status` — adapter power, discovering state
- `bluetooth.adapters` — list of adapters
- `bluetooth.devices` — all known devices with connection/pairing state
- `bluetooth.device.{mac}` — single device state

## Methods

- `bluetooth.set_powered(adapter: String, powered: bool)` — turn adapter on/off
- `bluetooth.start_discovery(adapter: String)` — start scanning
- `bluetooth.stop_discovery(adapter: String)` — stop scanning
- `bluetooth.connect(device: String)` — connect to device by MAC
- `bluetooth.disconnect(device: String)` — disconnect device
- `bluetooth.pair(device: String)` — initiate pairing
- `bluetooth.trust(device: String, trusted: bool)` — set trust state
- `bluetooth.forget(device: String)` — remove device and pairing data
- `bluetooth.set_alias(device: String, alias: String)` — set device friendly name
- `bluetooth.set_blocked(device: String, blocked: bool)` — block/unblock device
- `bluetooth.connect_profile(device: String, uuid: String)` — connect specific profile (e.g. A2DP)
- `bluetooth.disconnect_profile(device: String, uuid: String)` — disconnect specific profile

## Types

```rust
/// Major Bluetooth device class
enum BluetoothDeviceClass {
    Computer,
    Phone,
    NetworkAccessPoint,
    AudioVideo,
    Peripheral,
    Imaging,
    Wearable,
    Toy,
    Health,
    Uncategorized,
    Unknown(u8),
}

/// Bluetooth adapter state
struct BluetoothAdapterStatus {
    /// Adapter path (e.g. "hci0")
    id: String,
    address: String,
    alias: String,
    powered: bool,
    discoverable: bool,
    discovering: bool,
    pairable: bool,
}

/// Overall Bluetooth status, emitted on `bluetooth.status`
struct BluetoothStatus {
    /// Any adapter powered on?
    powered: bool,
    /// Any adapter discovering?
    discovering: bool,
    /// Number of connected devices
    connected_count: u32,
}

/// A Bluetooth device
struct BluetoothDevice {
    /// MAC address
    address: String,
    /// Display name (Alias if set, else Name, else address)
    name: String,
    /// BlueZ-suggested icon name
    icon: String,
    device_class: BluetoothDeviceClass,
    paired: bool,
    bonded: bool,
    trusted: bool,
    blocked: bool,
    connected: bool,
    /// Signal strength in dBm (None if not available)
    rssi: Option<i16>,
    /// Battery percentage (None if device doesn't report)
    battery: Option<u8>,
    /// Parent adapter ID
    adapter: String,
    /// Advertised service UUIDs
    uuids: Vec<String>,
}
```

## Icons

Adapter state:
- `bluetooth-active-symbolic` — Bluetooth on
- `bluetooth-disabled-symbolic` — Bluetooth off

Device types (from BlueZ `Icon` property or derived from class):
- `audio-headphones-symbolic` — headphones / earbuds
- `audio-headset-symbolic` — headset with mic
- `audio-speakers-symbolic` — speakers
- `input-keyboard-symbolic` — keyboard
- `input-mouse-symbolic` — mouse
- `input-gaming-symbolic` — game controller
- `input-tablet-symbolic` — drawing tablet
- `phone-symbolic` — phone
- `computer-symbolic` — computer
- `video-display-symbolic` — display / TV

All icons above are available in Adwaita icon theme.

## Crates

- `bluer` (0.16) — official BlueZ Rust bindings; async, type-safe, handles ObjectManager/Agent/GATT. Recommended over raw zbus for this provider.
- `zbus` (5) — alternative: raw D-Bus access if bluer doesn't cover a specific need

## Change Detection

**Fully reactive via D-Bus signals:**

- `ObjectManager.InterfacesAdded` — new device discovered, or battery interface appears on existing device
- `ObjectManager.InterfacesRemoved` — device removed or out of range
- `Device1.PropertiesChanged` — connection state, RSSI, paired, trusted, name changes
- `Adapter1.PropertiesChanged` — powered, discovering, discoverable changes
- `Battery1.PropertiesChanged` — battery percentage changes

**Recommended pattern:**
1. `GetManagedObjects()` for initial state
2. Subscribe to `InterfacesAdded`/`InterfacesRemoved` on ObjectManager
3. Subscribe to `PropertiesChanged` on each adapter and device
4. Maintain local cache, emit provider events on each signal

No polling needed — BlueZ is fully signal-driven.

## Features

- List Bluetooth adapters with power/discoverable/discovering state
- List all known devices with connection, pairing, trust, blocked state
- Device discovery (start/stop scanning)
- Pairing with agent support (passkey confirmation, PIN entry)
- Connect/disconnect individual devices or specific profiles
- Trust/block device management
- Forget (remove) devices with bond data cleanup
- Device battery level reporting (via Battery1 interface)
- Device class detection (audio, peripheral, phone, computer, etc.)
- RSSI signal strength monitoring during discovery
- Multiple adapter support
- BLE device support (address type, GATT services)
- Manufacturer and service advertisement data
- Audio profile management (A2DP, HSP/HFP via ConnectProfile)
- Discoverable mode control with timeout

## Notes

- BlueZ uses ObjectManager pattern — always prefer `GetManagedObjects()` over per-object queries for initial load
- `bluer` crate is the official binding and handles the ObjectManager pattern, Agent registration, and async streams natively
- Battery1 interface is only present on devices that report battery — check `InterfacesAdded` for its appearance
- The `Icon` property on Device1 provides BlueZ's suggestion for which icon to use — often accurate
- Audio profile switching (A2DP ↔ HFP) is handled at the PulseAudio/PipeWire card level, not directly through BlueZ
- `Trusted = true` allows a device to auto-connect when in range — important for headphones and keyboards
- RSSI is only available during active discovery scanning
- Peripheral device batteries (headsets, mice, keyboards) are also reported by the battery provider via UPower — consider cross-referencing to avoid duplicate data
