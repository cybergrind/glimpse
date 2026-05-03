# Packaging Guide

## Binaries

| Binary | Description | Install to |
|--------|-------------|------------|
| `glimpse-panel` | Wayland status panel | `/usr/bin/glimpse-panel` |
| `glimpse-wallpaper` | Wayland wallpaper and backdrop daemon | `/usr/bin/glimpse-wallpaper` |

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

### User service for glimpse-panel

```ini
# ~/.config/systemd/user/glimpse-panel.service
[Unit]
Description=Glimpse panel
PartOf=graphical-session.target
After=graphical-session-pre.target

[Service]
Type=simple
ExecStart=/usr/bin/glimpse-panel
Restart=on-failure
RestartSec=2

[Install]
WantedBy=graphical-session.target
```

**Note:** The packaged unit is a user service installed to `/usr/lib/systemd/user/glimpse-panel.service`.

### User service for glimpse-wallpaper

```ini
# ~/.config/systemd/user/glimpse-wallpaper.service
[Unit]
Description=Glimpse wallpaper
PartOf=graphical-session.target
After=graphical-session-pre.target

[Service]
Type=simple
ExecStart=/usr/bin/glimpse-wallpaper
Restart=on-failure
RestartSec=2

[Install]
WantedBy=graphical-session.target
```

**Note:** The packaged unit is a user service installed to `/usr/lib/systemd/user/glimpse-wallpaper.service`.
Start it together with `glimpse-shell` for shell-based sessions. Do not run it alongside older panel-owned background surfaces because both processes would own background layer surfaces.

## Configuration

| File | Location |
|------|----------|
| Panel config | `$XDG_CONFIG_HOME/glimpse/panel.toml` or `./panel.toml` |
| Shell and wallpaper config | `GLIMPSE_CONFIG`, `./config.toml`, or `$XDG_CONFIG_HOME/glimpse/config.toml` |
| User theme CSS | `$XDG_CONFIG_HOME/glimpse/themes/<name>.css` |
| Built-in structure/theme layers | embedded in `glimpse-panel` binary |

`glimpse-wallpaper` enables the optional backdrop by default. If `[backdrop]` is omitted, the daemon uses `wallpaper.path` for the backdrop image and applies the default `blur_radius = 24`.

## Arch Linux PKGBUILD notes

The AUR package is `glimpse` and consumes GitHub release binaries. It does not compile Rust code on user machines. Release archives are named:

```text
glimpse-<version>-x86_64.tar.zst
```

Each archive contains the final `/usr` tree:

```text
usr/bin/glimpse-panel
usr/bin/glimpse-wallpaper
usr/lib/systemd/user/glimpse-panel.service
usr/lib/systemd/user/glimpse-wallpaper.service
```

Local release helpers:

```bash
just binary-package      # Build dist/glimpse-<version>-x86_64.tar.zst
just github-release      # Upload the local archive to the matching GitHub release
just aur-publish         # Publish PKGBUILD + .SRCINFO to ssh://aur@aur.archlinux.org/glimpse.git
just release-local       # Tag, push, upload release asset, then publish AUR metadata
```

The tag-push GitHub workflow performs the same binary build and AUR publish path. Configure the repository secret `AUR_SSH_PRIVATE_KEY` with an SSH key accepted by the AUR account before expecting automatic AUR publication.

The source repository `PKGBUILD` keeps `b2sums_x86_64=('SKIP')` as a template. The local and GitHub release paths render the AUR copy with the actual BLAKE2 checksum for the uploaded binary archive.

### Manual source build commands

```bash
# Build
cargo build --release -p glimpse --bin glimpse-panel --no-default-features
cargo build --release -p glimpse-wallpaper

# Install binary
install -Dm755 target/release/glimpse-panel "$pkgdir/usr/bin/glimpse-panel"
install -Dm755 target/release/glimpse-wallpaper "$pkgdir/usr/bin/glimpse-wallpaper"

# Polkit
install -Dm755 data/glimpse-battery-helper "$pkgdir/usr/lib/glimpse/glimpse-battery-helper"
install -Dm644 data/io.glimpse.battery.policy "$pkgdir/usr/share/polkit-1/actions/io.glimpse.battery.policy"

# Systemd user service
install -Dm644 data/glimpse-panel.service "$pkgdir/usr/lib/systemd/user/glimpse-panel.service"
install -Dm644 data/glimpse-wallpaper.service "$pkgdir/usr/lib/systemd/user/glimpse-wallpaper.service"
```
