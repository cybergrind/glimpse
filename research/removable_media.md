# Removable Media Provider

**Source:** udisks2 D-Bus (`org.freedesktop.UDisks2`, system bus)

**What it does:** Detects USB drives, SD cards, optical media, and MTP devices. Provides mount/unmount/eject operations.

## System Interface

### org.freedesktop.UDisks2 (system bus)

Uses ObjectManager pattern at `/org/freedesktop/UDisks2`.

### org.freedesktop.UDisks2.Drive (object: `/org/freedesktop/UDisks2/drives/{id}`)

Properties:
- `Id: String` — unique drive identifier
- `Model: String` — drive model
- `Vendor: String` — manufacturer
- `Serial: String` — serial number
- `Size: u64` — total size in bytes
- `Media: String` — media type (e.g. "flash_sd", "optical_dvd", "thumb")
- `MediaAvailable: bool` — media inserted
- `MediaRemovable: bool` — media can be removed (USB, SD, optical)
- `Removable: bool` — drive itself is removable (USB stick)
- `ConnectionBus: String` — "usb", "sdio", "ieee1394", etc.
- `Ejectable: bool` — can be ejected (optical drives)
- `SortKey: String` — for UI ordering

Methods:
- `Eject(options: HashMap<String, Variant>)` — eject drive
- `PowerOff(options: HashMap<String, Variant>)` — power off USB port

### org.freedesktop.UDisks2.Block (object: `/org/freedesktop/UDisks2/block_devices/{name}`)

Properties:
- `Device: Vec<u8>` — device path (e.g. "/dev/sdb1")
- `Drive: ObjectPath` — parent drive
- `IdType: String` — filesystem type ("ext4", "vfat", "ntfs", "btrfs", "exfat")
- `IdLabel: String` — filesystem label
- `IdUUID: String` — filesystem UUID
- `Size: u64` — partition size in bytes
- `HintName: String` — suggested name for UI
- `HintIconName: String` — suggested icon

### org.freedesktop.UDisks2.Filesystem (same block object, additional interface)

Methods:
- `Mount(options: HashMap<String, Variant>) -> String` — mount and return mount point path
- `Unmount(options: HashMap<String, Variant>)` — unmount

Properties:
- `MountPoints: Vec<Vec<u8>>` — current mount points (byte arrays, null-terminated)
- `Size: u64` — filesystem size (if mounted)

### ObjectManager

- `GetManagedObjects()` — initial state
- `InterfacesAdded` — new drive/block/filesystem appeared (device plugged in)
- `InterfacesRemoved` — device removed (unplugged)

## Topics

- `removable_media.devices` — list of removable drives and their partitions
- `removable_media.device.{id}` — single device state (mounted, mount point, etc.)

## Methods

- `removable_media.mount(block_device: String) -> String` — mount a partition, returns mount point
- `removable_media.unmount(block_device: String)` — unmount a partition
- `removable_media.eject(drive: String)` — eject a drive (optical, USB)
- `removable_media.power_off(drive: String)` — safely power off USB port

## Types

```rust
/// Connection type of the drive
enum ConnectionBus {
    Usb,
    Sdio,
    Ieee1394,
    Other(String),
}

/// A removable drive
struct RemovableDrive {
    /// udisks2 drive ID
    id: String,
    vendor: String,
    model: String,
    serial: String,
    /// Total size in bytes
    size: u64,
    connection: ConnectionBus,
    removable: bool,
    ejectable: bool,
    media_available: bool,
    /// Partitions on this drive
    partitions: Vec<RemovablePartition>,
}

/// A partition/filesystem on a removable drive
struct RemovablePartition {
    /// Block device path (e.g. "/dev/sdb1")
    device: String,
    /// Filesystem type (e.g. "ext4", "vfat", "ntfs")
    fs_type: String,
    /// Filesystem label
    label: String,
    /// Filesystem UUID
    uuid: String,
    /// Partition size in bytes
    size: u64,
    /// Current mount point (None if not mounted)
    mount_point: Option<String>,
    /// Suggested icon name
    icon_name: Option<String>,
}
```

## Icons

- `drive-removable-media-symbolic` — generic removable device
- `drive-removable-media-usb-symbolic` — USB drive
- `media-removable-symbolic` — removable media
- `media-flash-sd-symbolic` — SD card (if available)
- `drive-optical-symbolic` — CD/DVD
- `media-eject-symbolic` — eject action

All icons above are available in Adwaita icon theme.

## Crates

- `zbus` (5) — D-Bus client for udisks2

## Change Detection

**Fully reactive via D-Bus signals:**
- `ObjectManager.InterfacesAdded` — device plugged in (new Drive, Block, Filesystem objects)
- `ObjectManager.InterfacesRemoved` — device unplugged
- `Filesystem.PropertiesChanged` — mount/unmount (MountPoints changes)
- `Drive.PropertiesChanged` — media inserted/removed

## Features

- Detect USB drives, SD cards, optical media on plug
- List partitions with filesystem type, label, size
- Mount/unmount partitions
- Eject drives (optical, USB safely)
- Power off USB ports
- Mount point reporting
- Auto-mount policy (future)
- Filesystem type detection (ext4, vfat, ntfs, exfat, btrfs)
- Drive vendor/model/serial info
- MTP device support (phones, cameras — via gvfs/udisks2 integration)
- SMART health info (future: via udisks2 ATA interface)

## Notes

- udisks2 handles permissions via polkit — user doesn't need root for mount/unmount
- `MountPoints` property is array of byte arrays (null-terminated) — parse carefully
- Filter out non-removable drives (internal SSDs/HDDs) by checking `Removable` and `ConnectionBus`
- Optical drives have `Ejectable=true` — show eject button
- `HintIconName` on Block gives udisks2's icon suggestion — use as primary, fall back to generic
