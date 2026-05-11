---
layout: home

hero:
  name: "Glimpse"
  text: "A polished desktop shell toolkit for Niri."
  tagline: "Panel, wallpaper, lock screen, night light, and idle behavior that feel like one desktop."
  actions:
    - theme: brand
      text: "Install Glimpse"
      link: /installation
    - theme: alt
      text: "Configure Your Desktop"
      link: /configuration

features:
  - title: "Make Niri Feel Complete"
    details: "Add the surrounding desktop pieces people expect from a daily Wayland session."
  - title: "Configure With Plain Files"
    details: "Keep panel layout, services, applets, wallpaper, and theme settings in readable TOML and CSS."
  - title: "Built For Daily Use"
    details: "Use calm defaults, focused controls, service integration, and custom themes for a desktop that feels finished."
---

## What You Get

Glimpse is for people who like Niri but still want the comfortable parts of a cohesive desktop.

| Component | What it does |
|---|---|
| **Shell** | Shows workspaces, status applets, tray items, weather, battery, network, media, notifications, and custom commands. |
| **Wallpaper** | Sets a color or image wallpaper, with an optional blurred backdrop. |
| **Lock screen** | Shows a themed lock screen with your wallpaper, clock, user picture, status buttons, and PAM authentication. |
| **Night light** | Warms the screen at sunset or on your chosen schedule. |
| **Idle rules** | Locks the session, turns displays off, suspends, or runs your own commands after inactivity. |
| **Custom applets** | Add launchers, menus, scripts, and live status widgets without rebuilding Glimpse. |

## Start Here

1. Install the package from [Installation](./installation.md).
2. Enable the shell, lock screen, night light, and idle services.
3. Add your first panel layout in [Configuration](./configuration.md).
4. Theme the [shell](./theming.md), [Lock Screen](./lock.md), and [Wallpaper](./wallpaper.md) for your setup.

## Good First Setup

Enable the main pieces:

```sh
systemctl --user enable --now glimpse-shell.service
systemctl --user enable --now glimpse-lock.service
systemctl --user enable --now glimpse-sunset.service
systemctl --user enable --now glimpse-idle.service
```

Then start with these pages:

| Goal | Page |
|---|---|
| Put useful things in the panel | [Configuration](./configuration.md) |
| Change colors and CSS | [Theming](./theming.md) |
| Use commands and scripts as applets | [Custom Applets](./custom-applets/) |
| Make the lock screen match your setup | [Lock](./lock.md) |
| Set wallpaper and blurred backdrop | [Wallpaper](./wallpaper.md) |
| Lock or blank displays after idle time | [Idle](./idle.md) |
| Warm the screen at night | [Sunset](./sunset.md) |
