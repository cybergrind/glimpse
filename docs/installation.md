# Installation

Glimpse is packaged for Arch-based systems as a prebuilt package. Install it once, then enable the pieces you want in your user session.

## Install From AUR

```sh
yay -S glimpse-desktop-bin
```

Use your preferred AUR helper if you do not use `yay`.

The package installs:

| Command | Purpose |
|---|---|
| `glimpse-shell` | Panel and shell surfaces. |
| `glimpse-wallpaper` | Wallpaper and blurred backdrop daemon. |
| `glimpse-lock` | Session lock screen and logind lock listener. |
| `glimpse-sunset` | Night-light daemon. |
| `glimpse-idle` | Idle policy daemon. |

The package also installs systemd user services and the default PAM service file for `glimpse-lock`.

## Enable Glimpse

For a normal Niri desktop, start with the shell, lock screen, night light, and idle rules:

```sh
systemctl --user enable --now glimpse-shell.service
systemctl --user enable --now glimpse-lock.service
systemctl --user enable --now glimpse-sunset.service
systemctl --user enable --now glimpse-idle.service
```

`glimpse-shell.service` wants `glimpse-wallpaper.service`, so starting the shell also starts the wallpaper daemon. Enable `glimpse-wallpaper.service` directly only if you want wallpaper without the shell.

## Check That It Is Running

```sh
systemctl --user status glimpse-shell.service
systemctl --user status glimpse-lock.service
systemctl --user status glimpse-sunset.service
systemctl --user status glimpse-idle.service
```

If a service fails, view its log:

```sh
journalctl --user -u glimpse-shell.service -e
```

Replace `glimpse-shell.service` with the service you are checking.

## First Config Files

Glimpse reads shared config from:

| Priority | Path |
|---|---|
| **1** | `GLIMPSE_CONFIG` environment variable |
| **2** | `./config.toml` in the current directory |
| **3** | `$XDG_CONFIG_HOME/glimpse/config.toml` |
| **4** | `$HOME/.config/glimpse/config.toml` when `XDG_CONFIG_HOME` is unset |

Most users create:

```txt
~/.config/glimpse/config.toml
```

Use that file for panel layout, applets, wallpaper, lock screen, idle rules, location, and night light. Use `~/.config/glimpse/themes/` for shell and lock CSS.

## Version Check

Each command supports `--version`:

```sh
glimpse-shell --version
glimpse-wallpaper --version
glimpse-lock --version
glimpse-sunset --version
glimpse-idle --version
```

## Common First Fixes

| Problem | Try this |
|---|---|
| Panel does not appear | Make sure Niri is running and check `journalctl --user -u glimpse-shell.service -e`. |
| Wallpaper does not show | Do not run another wallpaper tool at the same time. |
| Lock command does nothing | Enable `glimpse-lock.service`, then run `loginctl lock-session`. |
| Night light does not change color | Set a schedule in [Sunset](./sunset.md). The default is off. |
| Idle does nothing | Add listeners in [Idle](./idle.md). The default has no idle actions. |
