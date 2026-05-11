# Lock

The lock screen protects your session and gives your desktop a finished first impression. It can use your wallpaper, show the clock, display status buttons, and load your custom CSS.

`glimpse-lock` reads `[lock]` from `~/.config/glimpse/config.toml`.

## Enable Locking

```sh
systemctl --user enable --now glimpse-lock.service
```

Then test it:

```sh
loginctl lock-session
```

Use `loginctl lock-session` from keybindings and idle rules. Keep the service running so locking happens immediately.

## Starter CSS

Create starter CSS:

```sh
glimpse-lock --export-css
```

This writes:

```txt
~/.config/glimpse/themes/lock.css
```

## Example Config

```toml
[lock]
pam_service = "glimpse-lock"
css_path = "themes/lock.css"

[lock.background]
path = "/home/alex/Pictures/wallpapers/night-city.jpg"
fit = "cover"
blur_radius = 24
dim = 0.35

[lock.clock]
enabled = true
time_format = "%H:%M"
date_format = "%A, %B %-d"

[lock.controls]
buttons = ["wifi", "input", "weather", "battery", "power"]
```

If you do not set a lock background, Glimpse uses your wallpaper.

## Status Buttons

| Button | What it shows |
|---|---|
| `wifi` | Network status. |
| `input` | Current keyboard layout. |
| `weather` | Weather icon and temperature. |
| `battery` | Battery status. Percent is shown when running on battery. |
| `power` | Suspend, restart, and shutdown menu. |

Change the buttons or remove entries:

```toml
[lock.controls]
buttons = ["wifi", "battery", "power"]
```

## Theme Preview

Preview mode opens a normal window, reloads CSS while you edit it, and does not lock your real session.

```sh
glimpse-lock --preview
```

In preview mode:

| Password | Result |
|---|---|
| `valid` | Simulates a successful unlock. |
| Anything else | Simulates a failed unlock. |

## Custom CSS

Start with the exported CSS, then adjust spacing, fonts, and placement.

```css
.lock-entry {
  background: rgba(255, 255, 255, 0.20);
}
```

Keep custom CSS in `~/.config/glimpse/themes/lock.css`. The running preview reloads it when the file changes.

## Practical Notes

| Goal | Tip |
|---|---|
| Match wallpaper and lock screen | Leave `[lock.background]` unset in `config.toml`. |
| Build a glassy look | Use a blurred background and translucent entry styling. |
| Avoid accidental shutdown | Restart and shutdown ask for confirmation. |
| Test safely | Use `--preview`; it does not run real power actions. |
