# System Stats Provider

**Source:** /proc, /sys filesystem, statvfs syscall

**What it does:** Reports CPU usage, memory/swap usage, disk usage, temperatures, load average, uptime, and network I/O rates.

## System Interface

### /proc/stat (CPU usage)

Format: space-separated columns per CPU line.

```
cpu  user nice system idle iowait irq softirq steal guest guest_nice
cpu0 ...
cpu1 ...
```

All values in jiffies (1/100 second typically). Fields:
- `user` — user mode time
- `nice` — low-priority user mode time
- `system` — kernel mode time
- `idle` — idle time
- `iowait` — I/O wait time
- `irq` — hardware interrupt time
- `softirq` — software interrupt time
- `steal` — stolen time (virtualization)
- `guest` / `guest_nice` — guest VM time

CPU usage calculation (differential):
```
total = sum(all fields)
idle_total = idle + iowait
cpu_percent = (1 - (idle_delta / total_delta)) * 100
```

Requires two readings with ≥200ms interval.

### /proc/meminfo

Format: `Key: <value> kB`

Key fields:
- `MemTotal` — total usable RAM
- `MemAvailable` — estimate for new allocations (preferred over MemFree)
- `MemFree` — truly unused (don't use for "free memory")
- `Buffers` — block device buffers
- `Cached` — page cache
- `SwapTotal` — total swap
- `SwapFree` — unused swap

Used memory = MemTotal - MemAvailable

### /proc/loadavg

Format: `<1min> <5min> <15min> <running>/<total> <last_pid>`

Example: `0.37 0.47 0.39 1/839 31397`

### /proc/uptime

Format: `<uptime_seconds> <idle_seconds>`

Idle can exceed uptime on multi-core (sum across all cores).

### /proc/net/dev (network I/O)

Per-interface lines with receive/transmit columns:
- Receive: bytes, packets, errs, drop, fifo, frame, compressed, multicast
- Transmit: bytes, packets, errs, drop, fifo, colls, carrier, compressed

Rate calculation: take two readings, divide byte difference by time interval.

### /sys/class/thermal/thermal_zone{N}/temp

Single integer in millidegrees Celsius: `53000` = 53.0°C.

Also available: `/sys/class/thermal/thermal_zone{N}/type` — zone name (e.g. "x86_pkg_temp", "acpitz").

### statvfs (disk usage)

Per-mountpoint disk info via `statvfs()` syscall:
- `f_blocks * f_frsize` = total bytes
- `f_bavail * f_frsize` = available bytes (for unprivileged users)
- used = total - available

Read mount points from `/proc/mounts`.

## Topics

- `system_stats.cpu` — per-core and aggregate CPU usage percentage
- `system_stats.memory` — used/total RAM and swap
- `system_stats.disk` — per-mountpoint used/total
- `system_stats.temperatures` — thermal zone readings
- `system_stats.uptime` — seconds since boot
- `system_stats.load` — 1/5/15 minute load averages
- `system_stats.network_io` — per-interface RX/TX bytes and rates

## Methods

None (read-only provider).

## Types

```rust
/// Per-core and aggregate CPU usage
struct CpuStats {
    /// Aggregate CPU usage 0.0–100.0
    total_percent: f64,
    /// Per-core usage
    per_core: Vec<f64>,
    /// Number of cores
    core_count: u32,
}

/// Memory and swap usage
struct MemoryStats {
    /// Total RAM in bytes
    total: u64,
    /// Used RAM in bytes (total - available)
    used: u64,
    /// Available RAM in bytes
    available: u64,
    /// Total swap in bytes
    swap_total: u64,
    /// Used swap in bytes
    swap_used: u64,
}

/// A mounted filesystem
struct DiskStats {
    /// Mount point path (e.g. "/", "/home")
    mount_point: String,
    /// Filesystem type (e.g. "ext4", "btrfs")
    fs_type: String,
    /// Device path (e.g. "/dev/sda1")
    device: String,
    /// Total space in bytes
    total: u64,
    /// Used space in bytes
    used: u64,
    /// Available space in bytes
    available: u64,
}

/// A thermal sensor reading
struct TemperatureReading {
    /// Sensor label (e.g. "x86_pkg_temp", "acpitz")
    label: String,
    /// Temperature in degrees Celsius
    temp_celsius: f64,
}

/// System uptime
struct UptimeStats {
    /// Seconds since boot
    uptime_seconds: f64,
}

/// Load averages
struct LoadStats {
    /// 1-minute load average
    one: f64,
    /// 5-minute load average
    five: f64,
    /// 15-minute load average
    fifteen: f64,
    /// Currently running processes
    running: u32,
    /// Total processes
    total: u32,
}

/// Network interface I/O
struct NetworkIoStats {
    /// Interface name (e.g. "eth0", "wlan0")
    interface: String,
    /// Total received bytes
    rx_bytes: u64,
    /// Total transmitted bytes
    tx_bytes: u64,
    /// Receive rate in bytes/sec (computed from delta)
    rx_rate: f64,
    /// Transmit rate in bytes/sec (computed from delta)
    tx_rate: f64,
}
```

## Icons

- `utilities-system-monitor-symbolic` — system monitor
- `cpu-symbolic` — CPU (if available)
- `drive-harddisk-symbolic` — disk
- `network-transmit-receive-symbolic` — network activity

All icons above are available in Adwaita icon theme.

## Crates

- `sysinfo` — high-level: CPU, memory, disks, processes, temperatures, networks. Needs two `refresh()` calls for CPU accuracy.
- `procfs` (0.17) — direct /proc parsing: stat, meminfo, loadavg, uptime, net/dev, diskstats. Returns `Result`, no panics.
- `nix` — `statvfs()` syscall for disk usage

## Change Detection

**No signals available** — /proc and /sys are poll-only filesystems.

**Recommended intervals:**
- CPU usage: 1–2 seconds (needs two readings for delta)
- Memory: 2–5 seconds
- Disk: 10–30 seconds (slow-changing)
- Temperatures: 2–5 seconds
- Network I/O rates: 1–2 seconds (needs delta)
- Load average: 5–10 seconds
- Uptime: 60 seconds

The provider should use configurable poll intervals and only emit events when values change beyond a threshold (avoid flooding clients with identical data).

## Features

- Aggregate and per-core CPU usage percentage
- RAM usage (total, used, available)
- Swap usage
- Per-mountpoint disk usage (total, used, available, filesystem type)
- Thermal sensor readings (CPU, GPU, chipset, etc.)
- System uptime
- Load averages (1, 5, 15 minute)
- Per-interface network I/O rates (bytes/sec)
- Network total bytes transferred
- Process count (running/total)
- GPU usage and temperature (future: via /sys/class/drm or nvidia-smi)
- Fan speed (future: via /sys/class/hwmon)
- Top processes by CPU/memory (future)
- Disk I/O rates (future: via /proc/diskstats)

## Notes

- CPU usage requires two samples — first reading is meaningless on its own
- `MemAvailable` is the correct field for "free memory" — not `MemFree`
- `sysinfo` crate keeps an internal thread pool; disable with `default-features = false` if unwanted
- Temperature files may not exist on all systems (desktops without sensors)
- Filter `/proc/mounts` to exclude pseudo-filesystems (proc, sysfs, tmpfs, devtmpfs) for disk stats
- Network I/O counters are cumulative since boot — compute rates from deltas
- Consider debouncing: only emit event if value changed by >1% since last emission
