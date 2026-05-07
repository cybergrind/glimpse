# Idle

Idle rules decide what happens when you stop using the computer.

By default, Glimpse does nothing on idle. You choose the policy yourself.

## A Good Laptop Setup

Add this to `~/.config/glimpse/config.toml`:

```toml
[idle]
enabled = true
respect_inhibitors = true

[idle.profiles.ac]
listeners = [
  { timeout = 300, on_idle = "loginctl lock-session" },
  { timeout = 600, on_idle = "niri msg action power-off-monitors", on_resume = "niri msg action power-on-monitors" }
]

[idle.profiles.battery]
listeners = [
  { timeout = 180, on_idle = "loginctl lock-session" },
  { timeout = 300, on_idle = "niri msg action power-off-monitors", on_resume = "niri msg action power-on-monitors" }
]
```

Then enable the service:

```sh
systemctl --user enable --now glimpse-idle.service
```

## Listener Options

| Option | What it means |
|---|---|
| `timeout` | Seconds of no keyboard or mouse activity before the rule runs. |
| `on_idle` | Shell command to run after the timeout. |
| `on_resume` | Shell command to run when activity returns, but only if `on_idle` already ran. |
| `respect_inhibitors` | Optional per-rule override for apps that ask the desktop to stay awake. |

## AC And Battery Profiles

Glimpse can use different rules on charger and battery.

| Profile | Good for |
|---|---|
| `ac` | Longer timeouts while plugged in. |
| `battery` | Shorter timeouts to save power. |

When power state changes, Glimpse switches to the matching profile.

## Useful Commands

| Goal | Command |
|---|---|
| Lock the session | `loginctl lock-session` |
| Turn Niri monitors off | `niri msg action power-off-monitors` |
| Turn Niri monitors on | `niri msg action power-on-monitors` |
| Suspend | `systemctl suspend` |

## Inhibitors

Some apps ask the desktop not to idle. Video players, screen sharing, games, and presentation tools often do this.

This keeps those apps respected:

```toml
[idle]
respect_inhibitors = true
```

For one rule that should always run:

```toml
{ timeout = 900, on_idle = "loginctl lock-session", respect_inhibitors = false }
```
