# Network Provider & Applet (WiFi + Ethernet + VPN)

**Source:** NetworkManager D-Bus (`org.freedesktop.NetworkManager`, system bus)

Single provider for all network connectivity — WiFi, Ethernet, VPN. Reports status, lists access points, manages connections.

## System Interface

### org.freedesktop.NetworkManager (`/org/freedesktop/NetworkManager`)

Properties:
- `State: u32` — 0=Unknown, 10=Asleep, 20=Disconnected, 30=Disconnecting, 40=Connecting, 50=ConnectedLocal, 60=ConnectedSite, 70=ConnectedGlobal
- `Connectivity: u32` — 0=unknown, 1=none, 2=portal, 3=limited, 4=full
- `PrimaryConnection: ObjectPath` — default route connection
- `Metered: u32` — 0=unknown, 1=yes, 2=no, 3=guess-yes, 4=guess-no
- `ActiveConnections: Vec<ObjectPath>`
- `NetworkingEnabled: bool` (R/W)
- `WirelessEnabled: bool` (R/W)
- `WirelessHardwareEnabled: bool` (RO) — hardware RF-kill

Methods:
- `GetDevices() -> Vec<ObjectPath>`
- `Enable(enabled: bool)` — enable/disable all networking
- `ActivateConnection(connection, device, specific_object) -> ObjectPath`
- `AddAndActivateConnection(settings, device, specific_object) -> (ObjectPath, ObjectPath)` — create + connect
- `DeactivateConnection(active_connection)`
- `CheckConnectivity() -> u32`

Signals:
- `StateChanged(state: u32)`
- `DeviceAdded(device)` / `DeviceRemoved(device)`

### org.freedesktop.NetworkManager.Device (device path)

Properties:
- `DeviceType: u32` — 1=Ethernet, 2=WiFi, 5=Bluetooth, 14=Team, 15=Bridge, 16=VLAN, 29=WireGuard
- `State: u32` — 0=Unknown, 10=Unmanaged, 20=Unavailable, 30=Disconnected, 40=Prepare, 50=Config, 60=NeedAuth, 70=IPConfig, 80=IPCheck, 90=Secondaries, 100=Activated, 110=Deactivating, 120=Failed
- `Interface: String` — e.g. "eth0", "wlan0"
- `Ip4Config: ObjectPath`
- `Speed: u32` — Mb/s
- `ActiveConnection: ObjectPath`

### org.freedesktop.NetworkManager.Device.Wired

Properties:
- `Carrier: bool` — cable plugged in
- `Speed: u32` — Mb/s

### org.freedesktop.NetworkManager.Device.Wireless

Methods:
- `GetAllAccessPoints() -> Vec<ObjectPath>`
- `RequestScan(options: HashMap)`

Properties:
- `AccessPoints: Vec<ObjectPath>`
- `ActiveAccessPoint: ObjectPath`
- `Bitrate: u32` — Kb/s
- `LastScan: i64` — CLOCK_BOOTTIME ms

Signals:
- `AccessPointAdded(ap)` / `AccessPointRemoved(ap)`

### org.freedesktop.NetworkManager.AccessPoint (AP path)

Properties:
- `Ssid: Vec<u8>` — raw bytes
- `Strength: u8` — 0–100
- `Frequency: u32` — MHz
- `HwAddress: String` — BSSID
- `MaxBitrate: u32` — Kb/s
- `Flags: u32` — 0x01=PRIVACY, 0x02=WPS
- `WpaFlags: u32` / `RsnFlags: u32` — security flags

Security detection:
- WpaFlags == 0 && RsnFlags == 0 → Open
- (flags) & 0x400 → WPA3
- (flags) & 0x100 → WPA/WPA2 PSK
- (flags) & 0x200 → Enterprise
- Flags & 0x01 only → WEP

### org.freedesktop.NetworkManager.Connection.Active

Properties:
- `Id: String`, `Uuid: String`, `Type: String`
- `State: u32` — 0=unknown, 1=activating, 2=activated, 3=deactivating, 4=deactivated
- `Default: bool` — is default route
- `Vpn: bool`
- `Ip4Config: ObjectPath`

### org.freedesktop.NetworkManager.IP4Config

Properties:
- `AddressData: Vec<Dict>` — `{address: String, prefix: u32}`
- `Gateway: String`
- `NameserverData: Vec<Dict>` — `{address: String}`

### org.freedesktop.NetworkManager.Settings (`/org/freedesktop/NetworkManager/Settings`)

Methods:
- `ListConnections() -> Vec<ObjectPath>`
- `GetConnectionByUuid(uuid) -> ObjectPath`

### org.freedesktop.NetworkManager.Settings.Connection

Methods:
- `GetSettings() -> HashMap` — connection details (no secrets)
- `Delete()` — forget connection

### VPN connections

VPN connections are `ActiveConnection` objects with `Vpn: bool = true`.
Interface: `org.freedesktop.NetworkManager.VPN.Connection`
- `VpnState: u32` — 0=Unknown, 1=Prepare, 2=NeedAuth, 3=Connect, 4=GettingIP, 5=Activated, 6=Failed, 7=Disconnected

WireGuard appears as device type 29, not as VPN ActiveConnection.

## Topics

- `network.status` — connectivity, enabled, primary connection, metered, wifi enabled
- `network.wifi` — visible access points (deduplicated by SSID, strongest signal wins, sorted by signal desc)
- `network.connections` — all active connections (wifi, ethernet, VPN, wireguard)
- `network.devices` — network devices with state
- `network.saved_vpns` — all saved VPN/WireGuard connections (for popover list)

## Methods

- `network.set_wifi_enabled(enabled: bool)` — toggle WiFi radio
- `network.set_enabled(enabled: bool)` — toggle all networking
- `network.wifi_scan()` — trigger WiFi scan
- `network.connect(ssid: String, password: Option<String>)` — connect to WiFi (AddAndActivateConnection for new)
- `network.connect_uuid(uuid: String)` — activate saved connection by UUID
- `network.disconnect(uuid: String)` — deactivate connection
- `network.forget(uuid: String)` — delete saved connection

## Types

```rust
struct NetworkStatus {
    connectivity: String,        // "full", "limited", "portal", "none"
    enabled: bool,
    wifi_enabled: bool,
    wifi_hw_enabled: bool,
    primary_connection: String,  // connection name or ""
    primary_type: String,        // "wifi", "ethernet", "vpn"
    metered: bool,
    speed: u32,                  // Mb/s of primary connection
    icon: String,
}

struct WifiAccessPoint {
    ssid: String,
    strength: u8,               // 0–100
    frequency: u32,             // MHz
    security: String,           // "open", "wpa", "wpa2", "wpa3", "enterprise", "wep"
    connected: bool,
    saved: bool,                // has saved connection
    uuid: Option<String>,       // saved connection UUID
}

struct NetworkConnection {
    id: String,
    uuid: String,
    connection_type: String,    // "wifi", "ethernet", "vpn", "wireguard"
    device: String,             // interface name
    state: String,              // "activating", "activated", "deactivating"
    vpn: bool,
    ip4_address: Option<String>,
    gateway: Option<String>,
    dns: Vec<String>,
    speed: u32,                 // Mb/s
}

struct NetworkDevice {
    interface: String,
    device_type: String,        // "ethernet", "wifi"
    state: String,              // "connected", "unavailable"
    speed: u32,
    carrier: Option<bool>,      // ethernet only: cable plugged in
}

struct SavedVpn {
    id: String,
    uuid: String,
    connection_type: String,    // "vpn", "wireguard"
    active: bool,               // currently connected
    state: Option<String>,      // "activating", "activated" if active
}
```

## Icons

WiFi signal:
- `network-wireless-signal-excellent-symbolic` — 75–100%
- `network-wireless-signal-good-symbolic` — 50–75%
- `network-wireless-signal-ok-symbolic` — 25–50%
- `network-wireless-signal-weak-symbolic` — 1–25%
- `network-wireless-signal-none-symbolic` — 0%
- `network-wireless-disabled-symbolic` — WiFi off
- `network-wireless-acquiring-symbolic` — connecting

Ethernet:
- `network-wired-symbolic` — connected
- `network-wired-acquiring-symbolic` — connecting

VPN:
- `network-vpn-symbolic` — VPN active

General:
- `network-offline-symbolic` — no connectivity

## Change Detection

Fully reactive via D-Bus signals:
- `Manager.StateChanged` / `Manager.PropertiesChanged`
- `Device.Wireless.AccessPointAdded` / `AccessPointRemoved`
- `ActiveConnection.StateChanged`
- `AccessPoint.PropertiesChanged` — signal strength changes
- Debounce: 500ms coalescing for rapid updates

## Panel Applet

### Dual icon

Primary icon (connection type) + secondary VPN icon (visible only when VPN/WG active):

```
(wifi-icon) (vpn-icon)     ← WiFi + VPN
(wifi-icon)                ← WiFi only
(ethernet-icon) (vpn-icon) ← Ethernet + VPN
(offline-icon)             ← no connectivity
```

Primary icon logic:
- WiFi connected → signal strength icon
- WiFi connecting → `network-wireless-acquiring-symbolic`
- WiFi disabled → `network-wireless-disabled-symbolic`
- Ethernet connected → `network-wired-symbolic`
- Ethernet connecting → `network-wired-acquiring-symbolic`
- Offline → `network-offline-symbolic`

VPN icon: `network-vpn-symbolic`, visible when any VPN or WireGuard connection is activated.

### Tooltip

Auto-generated from connection state:
- WiFi: `"MyWiFi · 72 Mbps · 5 GHz"` + `" · Metered"` if metered + `" · VPN"` if VPN active
- Ethernet: `"Wired · 1000 Mbps"` + suffixes
- Offline: `"Network offline"`

Band derived from frequency: <3000 MHz = 2.4 GHz, <6000 MHz = 5 GHz, else 6 GHz.

Custom format via config: `"{ssid} · {speed} · {band}"` — keys: `{ssid}`, `{speed}`, `{type}`, `{ip}`, `{band}`, `{vpn_name}`

### Right-click on applet → connection details

PopoverMenu showing active connections with details. Each row is a button that copies the IP to clipboard on click.

```
┌───────────────────────────┐
│  MyWiFi                   │
│  IP: 192.168.1.42     [⎘] │
│  Gateway: 192.168.1.1     │
│  DNS: 1.1.1.1             │
│  Speed: 72 Mbps           │
│  ─────────────────────    │
│  Work VPN                 │
│  IP: 10.0.0.5         [⎘] │
└───────────────────────────┘
```

Only shows active connections. VPN section only when VPN active.

### Subscriptions

- `network.status` — icon, tooltip, hero
- `network.connections` — VPN icon visibility, right-click details
- `network.wifi` — popover AP list
- `network.devices` — popover ethernet section
- `network.saved_vpns` — popover VPN section

### Config

```toml
[applets.network]
extends = "network"
label_format = ""                          # "{ssid}", "{speed}", "" (icon only)
tooltip_format = ""                        # "" = auto, or "{ssid} · {speed} · {band}"
show_vpn_icon = true                       # secondary VPN icon in panel
settings_command = "nm-connection-editor"
```

## Popover Layout

```
┌──────────────────────────────────────────┐
│  (wifi-icon 32px)  Network               │  hero
│  MyWiFi · 72 Mbps · Metered             │
├──────────────────────────────────────────┤
│  WiFi                          [switch]  │  wifi toggle
│  ● MyWiFi                     72 Mbps    │  connected AP (accent)
│    CoffeeShop_5G                 🔒      │  encrypted, not connected
│    ┌─────────────────────────────────┐   │  inline password (expanded)
│    │ Password...        👁  [Connect]│   │
│    └─────────────────────────────────┘   │
│    Neighbors                     🔒      │
│    OpenNet                               │  open, not connected
│    [Scan]                                │
├──────────────────────────────────────────┤
│  Wired                        [switch]   │  ethernet toggle
│  ● enp3s0                    1000 Mbps   │  connected device
├──────────────────────────────────────────┤
│  VPN                                     │  all saved VPN/WG connections
│  ● Work VPN                              │  connected (accent)
│    Home VPN                              │  saved, not connected
├──────────────────────────────────────────┤
│  Network Settings                        │
└──────────────────────────────────────────┘
```

### Hero section

32px icon (matches primary connection type) + "Network" title + subtitle.

Subtitle:
- WiFi: `"MyWiFi · 72 Mbps"` + `" · Metered"` if metered
- Ethernet: `"Wired · 1000 Mbps"` + `" · Metered"` if metered
- Offline: `"Not connected"`

No master toggle in hero (WiFi and Wired have their own switches).

### WiFi section

- Toggle switch: `network.set_wifi_enabled(enabled)`. Disabled if `wifi_hw_enabled = false` (hardware RF-kill).
- AP list in `ScrolledWindow` (max height 300px), sorted by signal descending
- Each AP row: dimmed signal strength icon (16px) + SSID label + lock icon (if encrypted)
- Connected AP: accent bullet `●`, show speed
- Click connected AP → `network.disconnect(uuid)`
- Click saved AP → `network.connect_uuid(uuid)`
- Click unsaved encrypted AP → expand inline password entry below
- Click unsaved open AP → `network.connect(ssid, None)` directly
- Scan button at bottom, auto-triggers on popover open, stops on close
- Right-click saved AP → context menu: Forget

### Inline password entry

Appears below the clicked AP row:
- `gtk::Entry` with `visibility: false`, `input_purpose: PASSWORD`
- Eye toggle button to show/hide password
- Connect button
- Enter key submits
- Escape or clicking another AP collapses
- Spinner on AP row while connecting, collapse on success
- On failure: show error label below entry ("Authentication failed"), keep entry visible

### Ethernet section

Only visible if ethernet devices exist.
- Toggle switch: activates/deactivates the auto wired connection
- Show interface name + speed for connected devices
- If carrier is false: show "Cable unplugged" dimmed

### VPN section

Only visible if saved VPN/WireGuard connections exist.
- Lists ALL saved VPN/WG connections (from `network.saved_vpns`)
- Connected VPNs: accent bullet `●`
- Click connected → `network.disconnect(uuid)`
- Click not connected → `network.connect_uuid(uuid)`
- Spinner while activating

### Settings button

Launches `settings_command` (default: `nm-connection-editor`). Hidden if empty.

### Right-click context menus

WiFi AP (saved): Forget
VPN: (no context menu needed)
Ethernet: (no context menu)

## File Structure

```
glimpsed/src/providers/network.rs       — daemon provider
glimpse-panel/src/applets/network/
  mod.rs                                — module exports
  applet.rs                             — dual icon, tooltip, subscriptions
  popover.rs                            — popover UI, AP list, password entry
  config.rs                             — NetworkConfig
```

## Notes

- All on system bus
- SSID is bytes — use lossy UTF-8 conversion
- `WirelessHardwareEnabled = false` means physical RF-kill — software can't override, disable switch
- `AddAndActivateConnection` for new WiFi connections (saves + connects in one call)
- WireGuard is device type 29, not a VPN ActiveConnection — merge into unified VPN view
- NetworkManager may not be installed — provider should handle absence gracefully
- Single D-Bus connection shared across all network functionality
- AP deduplication: multiple BSSIDs with same SSID → keep strongest signal
- No "disconnected" labels anywhere — use bullet/accent for connected, absence for not connected
