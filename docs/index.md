---
layout: home

hero:
  name: "Glimpse"
  text: "A prettier desktop shell for Niri."
  tagline: "Panels, wallpaper, lock screen, night light, and idle behavior that feel like one polished desktop."
  actions:
    - theme: brand
      text: "Install Glimpse"
      link: /installation
    - theme: alt
      text: "Configure Your Desktop"
      link: /configuration

features:
  - title: "Make Niri Feel Complete"
    details: "Add the pieces people expect from a daily desktop: panels, applets, lock screen, wallpaper, idle rules, and night light."
  - title: "Configure With Plain Files"
    details: "Keep your setup in readable TOML files under ~/.config/glimpse, ready to copy, version, and share."
  - title: "Built For Good Taste"
    details: "Use clean defaults, custom CSS, image backdrops, and small focused controls to make a desktop worth posting."
---

## What You Get

Glimpse is for people who like Niri but still want the comfortable parts of a full desktop environment.

| Feature | What it does |
|---|---|
| **Panel** | Shows workspaces, status applets, tray items, weather, battery, network, media, and custom commands. |
| **Wallpaper** | Sets a color or image wallpaper, with an optional blurred backdrop for a softer look. |
| **Lock screen** | Shows a themed lock screen with your wallpaper, clock, user picture, and status buttons. |
| **Night light** | Warms the screen at sunset or on your chosen schedule. |
| **Idle rules** | Locks the session, turns displays off, or runs your own commands after inactivity. |
| **Custom applets** | Add launchers, menus, scripts, and live status widgets without rebuilding Glimpse. |

## Start Here

1. Install the package from [Installation](./installation.md).
2. Enable the panel and wallpaper.
3. Add your first panel layout in [Configuration](./configuration.md).
4. Theme the [Lock Screen](./lock.md), [Wallpaper](./wallpaper.md), and [shell](./theming.md) for your setup.

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
