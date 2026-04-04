# WiFi Provider

**Source:** NetworkManager D-Bus (`org.freedesktop.NetworkManager`, system bus)

**What it does:** Lists WiFi access points, manages connections (connect, disconnect, forget), controls WiFi radio state, and reports signal strength and security info.

## System Interface

### org.freedesktop.NetworkManager (object: `/org/freedesktop/NetworkManager`)

Methods:
- `GetDevices() -> Vec<ObjectPath>` — list all network devices
- `ActivateConnection(connection: ObjectPath, device: ObjectPath, specific_object: ObjectPath) -> ObjectPath` — activate a saved connection; `specific_object` = AP path or "/" for auto
- `AddAndActivateConnection(settings: HashMap<String, HashMap<String, Variant>>, device: ObjectPath, specific_object: ObjectPath) -> (ObjectPath, ObjectPath)` — create new connection and activate; returns (connection_path, active_connection_path)
- `DeactivateConnection(active_connection: ObjectPath)` — disconnect

Properties:
- `Devices: Vec<ObjectPath>`
- `ActiveConnections: Vec<ObjectPath>` — currently active connection paths
- `WirelessEnabled: bool` (R/W) — WiFi radio soft state
- `WirelessHardwareEnabled: bool` (RO) — hardware RF-kill state
- `NetworkingEnabled: bool`
- `State: u32` — NMState (see below)
- `PrimaryConnection: ObjectPath`
- `Metered: u32` — 0=unknown, 1=yes, 2=no, 3=guess-yes, 4=guess-no
- `Connectivity: u32` — 0=unknown, 1=none, 2=portal, 3=limited, 4=full

Signals:
- `StateChanged(state: u32)`
- `DeviceAdded(device: ObjectPath)`
- `DeviceRemoved(device: ObjectPath)`

NMState values:
- 0=Unknown, 10=Asleep, 20=Disconnected, 30=Disconnecting, 40=Connecting, 50=ConnectedLocal, 60=ConnectedSite, 70=ConnectedGlobal

### org.freedesktop.NetworkManager.Device.Wireless (object: device path)

Methods:
- `GetAllAccessPoints() -> Vec<ObjectPath>` — all visible APs including hidden
- `RequestScan(options: HashMap<String, Variant>)` — trigger WiFi scan; options can include `ssids` for targeted scan

Properties:
- `AccessPoints: Vec<ObjectPath>`
- `ActiveAccessPoint: ObjectPath` — "/" if not connected
- `Mode: u32` — 0=unknown, 1=adhoc, 2=infrastructure, 3=ap
- `Bitrate: u32` — current speed in Kb/s
- `LastScan: i64` — CLOCK_BOOTTIME milliseconds of last scan (-1 = never)
- `PermHwAddress: String` — permanent MAC
- `WirelessCapabilities: u32`

Signals:
- `AccessPointAdded(ap: ObjectPath)`
- `AccessPointRemoved(ap: ObjectPath)`

### org.freedesktop.NetworkManager.AccessPoint (object: AP path)

Properties:
- `Ssid: Vec<u8>` — network name as raw bytes
- `Strength: u8` — signal strength 0–100
- `Frequency: u32` — channel frequency in MHz
- `HwAddress: String` — BSSID (MAC address)
- `MaxBitrate: u32` — max speed in Kb/s
- `Bandwidth: u32` — channel bandwidth in MHz
- `Mode: u32` — 0=unknown, 1=adhoc, 2=infrastructure, 3=ap
- `LastSeen: i32` — CLOCK_BOOTTIME seconds (-1 = never)
- `Flags: u32` — AP flags (see below)
- `WpaFlags: u32` — WPA security flags
- `RsnFlags: u32` — RSN (WPA2/WPA3) security flags

AP flags (NM80211ApFlags):
- `0x01` = PRIVACY (requires auth/encryption)
- `0x02` = WPS
- `0x04` = WPS_PBC (push-button)
- `0x08` = WPS_PIN

Security flags (NM80211ApSecurityFlags) — used for both WpaFlags and RsnFlags:
- `0x001` = PAIR_WEP40
- `0x002` = PAIR_WEP104
- `0x004` = PAIR_TKIP
- `0x008` = PAIR_CCMP (AES)
- `0x010` = GROUP_WEP40
- `0x020` = GROUP_WEP104
- `0x040` = GROUP_TKIP
- `0x080` = GROUP_CCMP
- `0x100` = KEY_MGMT_PSK (WPA/WPA2 personal)
- `0x200` = KEY_MGMT_802_1X (enterprise)
- `0x400` = KEY_MGMT_SAE (WPA3 personal)
- `0x800` = KEY_MGMT_OWE (opportunistic, no password)

Determining security type:
- WpaFlags == 0 && RsnFlags == 0 → Open
- (WpaFlags | RsnFlags) & 0x400 → WPA3
- (WpaFlags | RsnFlags) & 0x100 → WPA/WPA2 PSK
- (WpaFlags | RsnFlags) & 0x200 → Enterprise (802.1X)
- Flags & 0x01 only → WEP (legacy)

### org.freedesktop.NetworkManager.Connection.Active (object: active connection path)

Properties:
- `Connection: ObjectPath` — settings connection path
- `Id: String` — connection name
- `Uuid: String`
- `Type: String` — "802-11-wireless", "802-3-ethernet", etc.
- `Devices: Vec<ObjectPath>`
- `State: u32` — 0=unknown, 1=activating, 2=activated, 3=deactivating, 4=deactivated
- `Default: bool` — is IPv4 default route
- `Ip4Config: ObjectPath`
- `Ip6Config: ObjectPath`
- `Vpn: bool`

Signals:
- `StateChanged(state: u32, reason: u32)`

### org.freedesktop.NetworkManager.Settings (object: `/org/freedesktop/NetworkManager/Settings`)

Methods:
- `ListConnections() -> Vec<ObjectPath>` — all saved connections
- `GetConnectionByUuid(uuid: String) -> ObjectPath`
- `AddConnection(settings: HashMap<String, HashMap<String, Variant>>) -> ObjectPath` — save new connection

Signals:
- `NewConnection(connection: ObjectPath)`
- `ConnectionRemoved(connection: ObjectPath)`

### org.freedesktop.NetworkManager.Settings.Connection (object: connection path)

Methods:
- `GetSettings() -> HashMap<String, HashMap<String, Variant>>` — get connection details (no secrets)
- `GetSecrets(setting_name: String) -> HashMap<String, HashMap<String, Variant>>` — get passwords
- `Delete()` — forget/remove connection
- `Update(settings: HashMap<String, HashMap<String, Variant>>)` — update and save

### Connection workflows

Connect to known network:
1. `Settings.ListConnections()` → find connection matching SSID
2. `Manager.ActivateConnection(connection_path, device_path, "/")` → activate
3. Monitor `ActiveConnection.StateChanged` for state 2 (activated)

Connect to new network with password:
1. Build settings dict with `802-11-wireless` (ssid), `802-11-wireless-security` (key-mgmt: "wpa-psk", psk: password), `ipv4` (method: "auto"), `ipv6` (method: "auto")
2. `Manager.AddAndActivateConnection(settings, device_path, ap_path)` → creates and connects
3. Monitor `ActiveConnection.StateChanged`

Forget network:
1. Find connection via `Settings.ListConnections()` + `GetSettings()`
2. `Connection.Delete()`

Enable/disable WiFi:
- Set `Manager.WirelessEnabled` property to true/false

## Topics

- `wifi.status` — WiFi enabled, hardware enabled, connected SSID, signal strength, connectivity
- `wifi.adapters` — list of WiFi devices
- `wifi.stations` — list of visible access points
- `wifi.known_networks` — saved connections
- `wifi.active_connection` — current connection details (IP, speed, signal)

## Methods

- `wifi.set_enabled(enabled: bool)` — enable/disable WiFi radio
- `wifi.scan()` — trigger a WiFi scan
- `wifi.connect(ssid: String, password: Option<String>)` — connect to network (creates connection if needed)
- `wifi.disconnect()` — disconnect from current network
- `wifi.forget(uuid: String)` — delete a saved connection by UUID

## Types

```rust
/// WiFi security type (derived from AP flags)
enum WifiSecurity {
    Open,
    Wep,
    WpaPsk,
    Wpa2Psk,
    Wpa3Sae,
    Enterprise,
    Owe,
}

/// WiFi adapter operating mode
enum WifiMode {
    Unknown,
    AdHoc,
    Infrastructure,
    AccessPoint,
}

/// Overall network connectivity
enum Connectivity {
    Unknown,
    None,
    Portal,
    Limited,
    Full,
}

/// WiFi radio state
struct WifiStatus {
    /// Software-controlled WiFi enable
    enabled: bool,
    /// Hardware RF-kill state (false = physically blocked)
    hardware_enabled: bool,
    /// Connected SSID (None if disconnected)
    connected_ssid: Option<String>,
    /// Signal strength 0–100 (None if disconnected)
    signal_strength: Option<u8>,
    /// Internet connectivity state
    connectivity: Connectivity,
    /// Whether connection is metered
    metered: bool,
}

/// A WiFi adapter
struct WifiAdapter {
    /// D-Bus device path
    path: String,
    /// Interface name (e.g. "wlan0")
    interface: String,
    /// Permanent MAC address
    hw_address: String,
    mode: WifiMode,
    /// Current bitrate in Kb/s
    bitrate: u32,
}

/// A visible WiFi access point
struct WifiAccessPoint {
    /// D-Bus object path
    path: String,
    /// Network name (decoded from bytes)
    ssid: String,
    /// Signal strength 0–100
    strength: u8,
    /// Channel frequency in MHz
    frequency: u32,
    /// BSSID (MAC address)
    hw_address: String,
    /// Max speed in Kb/s
    max_bitrate: u32,
    security: WifiSecurity,
    mode: WifiMode,
}

/// A saved WiFi connection
struct WifiKnownNetwork {
    /// Connection UUID
    uuid: String,
    /// Connection name
    id: String,
    /// SSID
    ssid: String,
    /// Whether auto-connect is enabled
    autoconnect: bool,
    /// Last used timestamp
    last_used: Option<u64>,
}

/// Active WiFi connection details
struct WifiActiveConnection {
    /// Connection name
    id: String,
    uuid: String,
    ssid: String,
    signal_strength: u8,
    /// Current speed in Kb/s
    bitrate: u32,
    /// IPv4 address (if assigned)
    ip4_address: Option<String>,
    /// IPv6 address (if assigned)
    ip6_address: Option<String>,
    /// Connection state
    state: WifiConnectionState,
}

/// Active connection state
enum WifiConnectionState {
    Unknown,
    Activating,
    Activated,
    Deactivating,
    Deactivated,
}
```

## Icons

Signal strength:
- `network-wireless-signal-excellent-symbolic` — 75–100%
- `network-wireless-signal-good-symbolic` — 50–75%
- `network-wireless-signal-ok-symbolic` — 25–50%
- `network-wireless-signal-weak-symbolic` — 1–25%
- `network-wireless-signal-none-symbolic` — 0% / disconnected

Status:
- `network-wireless-acquiring-symbolic` — connecting
- `network-wireless-connected-symbolic` — connected
- `network-wireless-encrypted-symbolic` — encrypted network
- `network-wireless-disabled-symbolic` — WiFi off (software)
- `network-wireless-hardware-disabled-symbolic` — RF-kill active
- `network-wireless-offline-symbolic` — no internet (captive portal)
- `network-wireless-hotspot-symbolic` — AP/hotspot mode

All icons above are available in Adwaita icon theme.

## Crates

- `zbus` (5) — D-Bus client for NetworkManager
- `networkmanager` (0.3) — NetworkManager D-Bus bindings (optional, can use raw zbus)

## Change Detection

**D-Bus signals (fully reactive):**
- `Manager.StateChanged` — overall network state
- `Manager.PropertiesChanged` — `WirelessEnabled`, `ActiveConnections` changes
- `Device.Wireless.AccessPointAdded` / `AccessPointRemoved` — scan results
- `Device.Wireless.PropertiesChanged` — `LastScan` for scan completion
- `ActiveConnection.StateChanged` — connection progress
- `Settings.NewConnection` / `ConnectionRemoved` — saved networks changed

**Signal strength:** No dedicated signal. Poll `AccessPoint.Strength` property every 1–2 seconds for real-time signal display, or use `PropertiesChanged` on the active AP.

## Features

- List all visible access points with SSID, signal strength, security type, frequency
- Connect to known (saved) networks
- Connect to new networks with password (WPA/WPA2/WPA3)
- Disconnect from current network
- Forget saved networks
- Enable/disable WiFi radio
- Trigger manual WiFi scan
- Report hardware RF-kill state
- Active connection details: IP address, bitrate, signal
- Internet connectivity detection (none/portal/limited/full)
- Metered connection detection
- WiFi adapter listing with capabilities
- Saved/known network management
- Captive portal detection
- WPA3 and OWE security support
- Hidden SSID scanning

## Notes

- All interfaces are on system bus, not session bus
- SSID is a byte array — may contain non-UTF8 data; display with lossy conversion
- `WirelessHardwareEnabled` = false means physical RF-kill switch is on — software cannot override
- `AddAndActivateConnection` both saves and connects in one call — preferred for new networks
- Signal strength poll interval should be 1–2 seconds for responsive UI
- NetworkManager may not be installed on all systems — provider should handle absence
- Shares NetworkManager D-Bus service with the network provider — coordinate to avoid duplicate D-Bus connections
