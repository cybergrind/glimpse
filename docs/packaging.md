# Packaging Guide

## Binaries

| Binary | Description | Install to |
|--------|-------------|------------|
| `glimpse-panel` | Wayland status panel | `/usr/bin/glimpse-panel` |
| `glimpse-shell` | Wayland shell | `/usr/bin/glimpse-shell` |
| `glimpse-idle` | Wayland idle daemon | `/usr/bin/glimpse-idle` |
| `glimpse-lock` | Wayland session lock screen and logind lock listener | `/usr/bin/glimpse-lock` |
| `glimpse-sunset` | Wayland night-light daemon | `/usr/bin/glimpse-sunset` |
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

## PAM

`glimpse-lock` authenticates through PAM. The default service name is `glimpse-lock`.

| Source | Install to |
|--------|------------|
| `data/pam.d/glimpse-lock` | `/etc/pam.d/glimpse-lock` |

## Systemd

### User service for glimpse-shell

```ini
# ~/.config/systemd/user/glimpse-shell.service
[Unit]
Description=Glimpse shell
PartOf=graphical-session.target
After=graphical-session-pre.target glimpse-wallpaper.service
Wants=glimpse-wallpaper.service

[Service]
Type=simple
ExecStart=/usr/bin/glimpse-shell
Restart=on-failure
RestartSec=2

[Install]
WantedBy=graphical-session.target
```

**Note:** The packaged unit is a user service installed to `/usr/lib/systemd/user/glimpse-shell.service`.
It wants `glimpse-wallpaper.service` so starting the shell also starts the wallpaper daemon.

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

### User service for glimpse-sunset

```ini
# ~/.config/systemd/user/glimpse-sunset.service
[Unit]
Description=Glimpse sunset
PartOf=graphical-session.target
After=graphical-session-pre.target

[Service]
Type=simple
ExecStart=/usr/bin/glimpse-sunset
Restart=on-failure
RestartSec=2

[Install]
WantedBy=graphical-session.target
```

**Note:** The packaged unit is a user service installed to `/usr/lib/systemd/user/glimpse-sunset.service`.
It is standalone and is not wanted by `glimpse-shell.service`. Enable it explicitly:

```bash
systemctl --user enable --now glimpse-sunset.service
```

Do not run `glimpse-sunset` alongside the old panel-owned night-light service because Wayland gamma control has one owner per output.

### User service for glimpse-idle

```ini
# ~/.config/systemd/user/glimpse-idle.service
[Unit]
Description=Glimpse idle
PartOf=graphical-session.target
After=graphical-session-pre.target

[Service]
Type=simple
ExecStart=/usr/bin/glimpse-idle
Restart=on-failure
RestartSec=2

[Install]
WantedBy=graphical-session.target
```

**Note:** The packaged unit is a user service installed to `/usr/lib/systemd/user/glimpse-idle.service`.
It is standalone and is not wanted by `glimpse-shell.service`. Enable it explicitly:

```bash
systemctl --user enable --now glimpse-idle.service
```

`glimpse-idle` consumes Wayland idle notifications and runs listener shell commands from `[idle]`.
Do not run it alongside another idle policy daemon such as `hypridle` or `swayidle` unless their timeouts and commands are intentionally coordinated.

### User service for glimpse-lock

```ini
# ~/.config/systemd/user/glimpse-lock.service
[Unit]
Description=Glimpse lock screen
PartOf=graphical-session.target
After=graphical-session-pre.target

[Service]
Type=simple
ExecStart=/usr/bin/glimpse-lock
Restart=on-failure
RestartSec=2

[Install]
WantedBy=graphical-session.target
```

**Note:** The packaged unit is installed to `/usr/lib/systemd/user/glimpse-lock.service`.
Enable it when you want logind integration:

```bash
systemctl --user enable --now glimpse-lock.service
```

With this service running, `loginctl lock-session` asks logind to emit the current session's `Lock` signal, and the resident `glimpse-lock` process acquires the Wayland session lock in-process.
`glimpse-lock` updates logind's `LockedHint` after the Wayland session lock is acquired and clears it after unlock.
Compositor key bindings and idle policies should trigger `loginctl lock-session`; they should not start another `glimpse-lock` process while the service is running.
For security, `glimpse-lock` ignores logind `Unlock` requests; unlocking requires successful local PAM authentication or a compositor-driven session-lock release.

## Configuration

| File | Location |
|------|----------|
| Panel config | `$XDG_CONFIG_HOME/glimpse/panel.toml` or `./panel.toml` |
| Shell, wallpaper, sunset, and idle config | `GLIMPSE_CONFIG`, `./config.toml`, or `$XDG_CONFIG_HOME/glimpse/config.toml` |
| Lock config | `GLIMPSE_LOCK_CONFIG`, `./lock.toml`, or `$XDG_CONFIG_HOME/glimpse/lock.toml` |
| User theme CSS | `$XDG_CONFIG_HOME/glimpse/themes/<name>.css` |
| Lock screen CSS | `$XDG_CONFIG_HOME/glimpse/themes/lock.css` |
| Built-in structure/theme layers | embedded in `glimpse-panel` binary |

`glimpse-wallpaper` enables the optional backdrop by default. If `[backdrop]` is omitted, the daemon uses `wallpaper.path` for the backdrop image and applies the default `blur_radius = 24`.

`glimpse-sunset` reads the shared `[night_light]` block. `schedule = "off"` disables gamma changes, `schedule = "schedule"` uses `start_time`/`end_time`, and `schedule = "automatic"` uses the shared location service. Configure static coordinates under `[location]` when GeoClue is unavailable or undesired.

`glimpse-idle` reads the shared `[idle]` block. If it is omitted, the daemon starts with no listener policies, so it does not lock, blank displays, suspend, or run shell commands until listeners are configured:

```toml
[idle]
enabled = true
respect_inhibitors = true

[idle.profiles.ac]
listeners = []

[idle.profiles.battery]
listeners = []
```

Example Niri policy:

```toml
[idle.profiles.ac]
listeners = [
  { timeout = 300, on_idle = "loginctl lock-session" },
  { timeout = 600, on_idle = "niri msg action power-off-monitors", on_resume = "niri msg action power-on-monitors" },
  { timeout = 1800, on_idle = "systemctl suspend" },
]

[idle.profiles.battery]
listeners = [
  { timeout = 300, on_idle = "loginctl lock-session" },
  { timeout = 600, on_idle = "niri msg action power-off-monitors", on_resume = "niri msg action power-on-monitors" },
  { timeout = 900, on_idle = "systemctl suspend" },
]
```

Each listener command is executed through `/bin/sh -c`. `on_resume` runs only after that listener has fired `on_idle`.
Set `respect_inhibitors` on a listener to override the global value.

`glimpse-lock` reads `lock.toml`. It still uses the shared `[wallpaper]` block from `config.toml` as a fallback when `lock.toml` omits background color or path.

```toml
pam_service = "glimpse-lock"
css_path = "themes/lock.css"

[background]
color = "#101010"
path = "/path/to/lock.png"
fit = "cover"
blur_radius = 0
dim = 0.35
```

All `background` fields are optional. If `background.color` or `background.path` is absent, `glimpse-lock` falls back to `[wallpaper]` from the shared config. Custom CSS from `themes/lock.css` is watched and loaded over embedded defaults.

The default `pam_service = "glimpse-lock"` expects `/etc/pam.d/glimpse-lock`, which the binary package installs. Override `pam_service` only if you intentionally want to use a different PAM stack.

For lock theme work without taking a real session lock, run:

```bash
GLIMPSE_LOCK_CONFIG=/path/to/lock.toml cargo run -p glimpse-lock -- --preview
```

Preview mode opens a normal GTK window, uses the same lock background and CSS watchers, and simulates authentication. Password `valid` succeeds; any other non-empty password fails.

## Arch Linux PKGBUILD notes

The AUR package is `glimpse-desktop-bin` and consumes GitHub release binaries. It does not compile Rust code on user machines. Release archives are named:

```text
glimpse-<version>-x86_64.tar.zst
```

Each archive contains the final `/usr` tree and the default PAM service file:

```text
etc/pam.d/glimpse-lock
usr/bin/glimpse-panel
usr/bin/glimpse-lock
usr/bin/glimpse-shell
usr/bin/glimpse-idle
usr/bin/glimpse-sunset
usr/bin/glimpse-wallpaper
usr/lib/systemd/user/glimpse-lock.service
usr/lib/systemd/user/glimpse-shell.service
usr/lib/systemd/user/glimpse-idle.service
usr/lib/systemd/user/glimpse-sunset.service
usr/lib/systemd/user/glimpse-wallpaper.service
```

Local release helpers:

```bash
just binary-package      # Build dist/glimpse-<version>-x86_64.tar.zst
just github-release      # Upload the local archive to the matching GitHub release
just aur-publish         # Publish PKGBUILD + .SRCINFO for the uploaded GitHub release asset
just release-local       # Tag, push, upload release asset, then publish AUR metadata
```

The tag-push GitHub workflow performs the same binary build and AUR publish path. Configure the repository secret `AUR_SSH_PRIVATE_KEY` with an SSH key accepted by the AUR account before expecting automatic AUR publication.

The source repository `PKGBUILD` keeps `b2sums_x86_64=('SKIP')` as a template. The local and GitHub release paths render the AUR copy with the actual BLAKE2 checksum for the uploaded binary archive.

### Manual source build commands

```bash
# Build
cargo build --release -p glimpse --bin glimpse-panel --no-default-features
cargo build --release -p glimpse-lock
cargo build --release -p glimpse-shell
cargo build --release -p glimpse-idle
cargo build --release -p glimpse-sunset
cargo build --release -p glimpse-wallpaper

# Install binary
install -Dm755 target/release/glimpse-panel "$pkgdir/usr/bin/glimpse-panel"
install -Dm755 target/release/glimpse-lock "$pkgdir/usr/bin/glimpse-lock"
install -Dm755 target/release/glimpse-shell "$pkgdir/usr/bin/glimpse-shell"
install -Dm755 target/release/glimpse-idle "$pkgdir/usr/bin/glimpse-idle"
install -Dm755 target/release/glimpse-sunset "$pkgdir/usr/bin/glimpse-sunset"
install -Dm755 target/release/glimpse-wallpaper "$pkgdir/usr/bin/glimpse-wallpaper"

# Polkit
install -Dm755 data/glimpse-battery-helper "$pkgdir/usr/lib/glimpse/glimpse-battery-helper"
install -Dm644 data/io.glimpse.battery.policy "$pkgdir/usr/share/polkit-1/actions/io.glimpse.battery.policy"

# PAM
install -Dm644 data/pam.d/glimpse-lock "$pkgdir/etc/pam.d/glimpse-lock"

# Systemd user service
install -Dm644 data/glimpse-lock.service "$pkgdir/usr/lib/systemd/user/glimpse-lock.service"
install -Dm644 data/glimpse-shell.service "$pkgdir/usr/lib/systemd/user/glimpse-shell.service"
install -Dm644 data/glimpse-idle.service "$pkgdir/usr/lib/systemd/user/glimpse-idle.service"
install -Dm644 data/glimpse-sunset.service "$pkgdir/usr/lib/systemd/user/glimpse-sunset.service"
install -Dm644 data/glimpse-wallpaper.service "$pkgdir/usr/lib/systemd/user/glimpse-wallpaper.service"
```
