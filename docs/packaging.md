# Packaging Guide

## Binaries

| Binary | Description | Install to |
|--------|-------------|------------|
| `glimpsed` | System service daemon | `/usr/bin/glimpsed` |
| `glimpsectl` | CLI/TUI client tool | `/usr/bin/glimpsectl` |
| `glimpse-panel` | Wayland status panel | `/usr/bin/glimpse-panel` |

## Polkit

### Battery charge threshold

The battery provider can set the charge end threshold via sysfs. This requires root, so it uses polkit for authorization.

**Files:**

| Source | Install to |
|--------|------------|
| `data/io.glimpse.battery.policy` | `/usr/share/polkit-1/actions/io.glimpse.battery.policy` |
| `data/glimpse-battery-helper` | `/usr/lib/glimpse/glimpse-battery-helper` |

**Permissions:**

- `glimpse-battery-helper` must be owned by root and executable (`root:root 755`)
- The polkit policy references the helper path via `org.freedesktop.policykit.exec.path`

**Policy behavior:**

- Active desktop session: allowed without password (`<allow_active>yes</allow_active>`)
- Inactive session: requires admin password
- Remote/non-session: requires admin password

**Install commands:**

```bash
install -Dm755 data/glimpse-battery-helper /usr/lib/glimpse/glimpse-battery-helper
install -Dm644 data/io.glimpse.battery.policy /usr/share/polkit-1/actions/io.glimpse.battery.policy
```

### Adding more polkit actions

Future providers that need root (e.g. airplane mode via rfkill) should follow the same pattern:
1. Add a policy XML to `data/` with action ID `io.glimpse.<provider>.<action>`
2. Add a minimal helper script to `data/` that does the privileged operation
3. Daemon tries direct access first, falls back to `pkexec` with the helper

## Systemd

### User service for glimpsed

```ini
# ~/.config/systemd/user/glimpsed.service
[Unit]
Description=Glimpse system service daemon
After=dbus.service

[Service]
Type=simple
ExecStart=/usr/bin/glimpsed
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

**Note:** `glimpsed` runs as a user service (not system), communicating over a Unix socket at `$XDG_RUNTIME_DIR/glimpsed.sock`.

## Socket path

Default: `$XDG_RUNTIME_DIR/glimpsed.sock`

Override: set `GLIMPSED_SOCKET` environment variable.

## Configuration

| File | Location |
|------|----------|
| Panel config | `$XDG_CONFIG_HOME/glimpse/panel.toml` or `./config.toml` |
| Theme CSS | `$XDG_CONFIG_HOME/glimpse/theme.css` or `./theme.css` |

## Arch Linux PKGBUILD notes

```bash
# Build
cargo build --release --workspace

# Install binaries
install -Dm755 target/release/glimpsed "$pkgdir/usr/bin/glimpsed"
install -Dm755 target/release/glimpsectl "$pkgdir/usr/bin/glimpsectl"
install -Dm755 target/release/glimpse-panel "$pkgdir/usr/bin/glimpse-panel"

# Polkit
install -Dm755 data/glimpse-battery-helper "$pkgdir/usr/lib/glimpse/glimpse-battery-helper"
install -Dm644 data/io.glimpse.battery.policy "$pkgdir/usr/share/polkit-1/actions/io.glimpse.battery.policy"

# Systemd user service
install -Dm644 data/glimpsed.service "$pkgdir/usr/lib/systemd/user/glimpsed.service"
```
