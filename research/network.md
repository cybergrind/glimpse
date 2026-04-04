# Network Provider

**Source:** NetworkManager D-Bus (`org.freedesktop.NetworkManager`, system bus)

**What it does:** Reports general network connectivity, Ethernet status, VPN connections, DNS info, and metered connection state. Complements the WiFi provider which handles wireless-specific features.

## System Interface

Uses the same NetworkManager D-Bus service as the WiFi provider. See `research/wifi.md` for the full interface reference. This provider focuses on non-wireless aspects.

### org.freedesktop.NetworkManager (object: `/org/freedesktop/NetworkManager`)

Key properties for this provider:
- `State: u32` — overall NM state (see NMState below)
- `Connectivity: u32` — internet connectivity (0=unknown, 1=none, 2=portal, 3=limited, 4=full)
- `PrimaryConnection: ObjectPath` — default route connection
- `Metered: u32` — 0=unknown, 1=yes, 2=no, 3=guess-yes, 4=guess-no
- `ActiveConnections: Vec<ObjectPath>` — all active connections
- `NetworkingEnabled: bool` (R/W) — master networking switch

NMState values:
- 0=Unknown, 10=Asleep, 20=Disconnected, 30=Disconnecting, 40=Connecting, 50=ConnectedLocal, 60=ConnectedSite, 70=ConnectedGlobal

Methods:
- `Enable(enabled: bool)` — enable/disable all networking
- `CheckConnectivity() -> u32` — force connectivity check

### org.freedesktop.NetworkManager.Device (object: device path)

Common properties:
- `DeviceType: u32` — 1=Ethernet, 2=WiFi, 5=Bluetooth, 6=OLPC, 7=WiMax, 8=Modem, 13=Generic, 14=Team, 15=Bridge, 16=VLAN, 29=WireGuard
- `State: u32` — device state (see NMDeviceState below)
- `Interface: String` — interface name (e.g. "eth0", "enp3s0")
- `Ip4Config: ObjectPath` — IPv4 configuration
- `Ip6Config: ObjectPath` — IPv6 configuration
- `HwAddress: String` — MAC address
- `Speed: u32` — link speed in Mb/s
- `Autoconnect: bool` (R/W)
- `ActiveConnection: ObjectPath`

NMDeviceState values:
- 0=Unknown, 10=Unmanaged, 20=Unavailable, 30=Disconnected, 40=Prepare, 50=Config, 60=NeedAuth, 70=IPConfig, 80=IPCheck, 90=Secondaries, 100=Activated, 110=Deactivating, 120=Failed

### org.freedesktop.NetworkManager.Device.Wired (Ethernet-specific)

Properties:
- `Carrier: bool` — cable plugged in
- `HwAddress: String` — MAC
- `PermHwAddress: String` — permanent MAC
- `Speed: u32` — link speed in Mb/s
- `S390Subchannels: Vec<String>` — s390 only

### org.freedesktop.NetworkManager.IP4Config / IP6Config (object: config path)

Properties:
- `Addresses: Vec<Vec<u32>>` — deprecated, use AddressData
- `AddressData: Vec<HashMap<String, Variant>>` — each with "address" (String) and "prefix" (u32)
- `Gateway: String` — default gateway
- `Nameservers: Vec<u32>` — DNS servers (IPv4 as u32, network byte order)
- `NameserverData: Vec<HashMap<String, Variant>>` — DNS with "address" (String)
- `Domains: Vec<String>` — search domains
- `DnsOptions: Vec<String>`
- `DnsPriority: i32`

### VPN connections

VPN connections appear as `ActiveConnection` objects with `Vpn: bool = true`.

Interface: `org.freedesktop.NetworkManager.VPN.Connection`
Properties:
- `VpnState: u32` — 0=Unknown, 1=Prepare, 2=NeedAuth, 3=Connect, 4=GettingIPConfig, 5=Activated, 6=Failed, 7=Disconnected
- `Banner: String` — VPN server banner message

VPN types are identified by `Type` property on the connection settings: "vpn", "wireguard", etc.

## Topics

- `network.status` — overall connectivity, primary connection, metered state
- `network.connections` — list of active connections (ethernet, VPN, etc.)
- `network.devices` — list of network devices with state and speed
- `network.vpn` — active VPN connections
- `network.dns` — current DNS configuration

## Methods

- `network.set_enabled(enabled: bool)` — enable/disable all networking
- `network.connect(uuid: String)` — activate a saved connection by UUID
- `network.disconnect(active_connection: String)` — deactivate a connection
- `network.check_connectivity() -> Connectivity` — force connectivity check

## Types

```rust
/// Overall network connectivity state
enum Connectivity {
    Unknown,
    None,
    Portal,
    Limited,
    Full,
}

/// Network device type
enum NetworkDeviceType {
    Ethernet,
    Wifi,
    Bluetooth,
    Modem,
    Bridge,
    Vlan,
    WireGuard,
    Other(u32),
}

/// Overall network status, emitted on `network.status`
struct NetworkStatus {
    connectivity: Connectivity,
    /// Whether networking is enabled
    enabled: bool,
    /// Primary connection name (None if disconnected)
    primary_connection: Option<String>,
    /// Whether primary connection is metered
    metered: bool,
}

/// A network device
struct NetworkDevice {
    /// Interface name (e.g. "eth0", "enp3s0")
    interface: String,
    device_type: NetworkDeviceType,
    /// Device state
    state: NetworkDeviceState,
    /// Link speed in Mb/s (0 if unknown)
    speed: u32,
    /// MAC address
    hw_address: String,
    /// Cable plugged in (Ethernet only)
    carrier: Option<bool>,
    /// Active connection name
    active_connection: Option<String>,
}

/// Device state (simplified)
enum NetworkDeviceState {
    Unknown,
    Unmanaged,
    Unavailable,
    Disconnected,
    Connecting,
    Connected,
    Deactivating,
    Failed,
}

/// An active network connection
struct NetworkConnection {
    id: String,
    uuid: String,
    /// Connection type: "802-3-ethernet", "vpn", "wireguard", etc.
    connection_type: String,
    /// Device interface name
    device: String,
    /// Is this a VPN connection?
    vpn: bool,
    /// Is this the default route?
    is_default: bool,
    /// IPv4 address
    ip4_address: Option<String>,
    /// IPv6 address
    ip6_address: Option<String>,
    /// Default gateway
    gateway: Option<String>,
}

/// VPN connection state
enum VpnState {
    Unknown,
    Prepare,
    NeedAuth,
    Connecting,
    GettingIpConfig,
    Activated,
    Failed,
    Disconnected,
}

/// DNS configuration, emitted on `network.dns`
struct DnsConfig {
    /// DNS server addresses
    servers: Vec<String>,
    /// Search domains
    domains: Vec<String>,
}
```

## Icons

Connectivity:
- `network-wired-symbolic` — Ethernet connected
- `network-wired-disconnected-symbolic` — Ethernet disconnected
- `network-wired-no-route-symbolic` — local only
- `network-wired-acquiring-symbolic` — connecting

VPN:
- `network-vpn-symbolic` — VPN active
- `network-vpn-acquiring-symbolic` — VPN connecting
- `network-vpn-disconnected-symbolic` — VPN off

General:
- `network-offline-symbolic` — no connectivity
- `network-error-symbolic` — connection error

All icons above are available in Adwaita icon theme.

## Crates

- `zbus` (5) — D-Bus client for NetworkManager
- `networkmanager` (0.3) — NetworkManager D-Bus bindings (optional)

## Change Detection

**Fully reactive via D-Bus signals:**

- `Manager.StateChanged(state: u32)` — overall connectivity change
- `Manager.PropertiesChanged` — `ActiveConnections`, `PrimaryConnection`, `Connectivity`, `Metered`, `NetworkingEnabled`
- `Device.PropertiesChanged` — `State`, `Carrier`, `Speed`, `ActiveConnection`
- `ActiveConnection.StateChanged(state: u32, reason: u32)` — connection progress
- `VPN.Connection.PropertiesChanged` — VPN state changes

No polling needed.

## Features

- Overall network connectivity state (none/portal/limited/full)
- Metered connection detection
- Ethernet device listing with carrier detect (cable plugged in)
- Link speed reporting
- Active connection listing (Ethernet, VPN, bridge, etc.)
- VPN connection status and state tracking
- IPv4/IPv6 address reporting
- Gateway and DNS configuration
- Enable/disable all networking
- Force connectivity check
- Device hotplug detection
- Multiple connection types: Ethernet, WireGuard, bridges, VLANs, modems

## Notes

- Shares NetworkManager D-Bus service with WiFi provider — coordinate to avoid duplicate connections
- VPN connections are regular ActiveConnection objects with `Vpn: bool = true`
- WireGuard appears as its own device type (29), not as a VPN connection
- `Carrier` property on wired devices detects physical cable state — useful for "cable unplugged" indication
- DNS config comes from IP4Config/IP6Config objects on the active connection
- NetworkManager may not be installed on all systems — provider should handle absence
