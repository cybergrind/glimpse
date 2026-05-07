# Installation

Glimpse is packaged for Arch-based systems as a prebuilt package. You install it once, then enable the pieces you want in your user session.

## Install From AUR

```sh
yay -S glimpse-desktop-bin
```

Use your preferred AUR helper if you do not use `yay`.

## Enable Glimpse

For a normal Niri desktop, start with the shell, lock screen, night light, and idle rules:

```sh
systemctl --user enable --now glimpse-shell.service
systemctl --user enable --now glimpse-lock.service
systemctl --user enable --now glimpse-sunset.service
systemctl --user enable --now glimpse-idle.service
```

The wallpaper starts with the shell, so most users do not need to enable it separately.

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

Glimpse reads config from:

| File | Purpose |
|---|---|
| `~/.config/glimpse/config.toml` | Panel, applets, idle, location, and night light. |
| `~/.config/glimpse/wallpaper.toml` | Wallpaper and backdrop. |
| `~/.config/glimpse/lock.toml` | Lock screen layout, controls, background, and clock. |
| `~/.config/glimpse/themes/lock.css` | Lock screen styling. |

You can start without writing every file. Glimpse has defaults, and each feature page shows a copyable example.

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
