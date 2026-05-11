# Theming

Glimpse is meant to look professional: calm, polished, readable, and free of noisy effects. Theme it like a desktop you can use all day, not like a demo.

## Theme Files

| File | Purpose |
|---|---|
| `~/.config/glimpse/themes/<name>.css` | Panel, applets, popovers, and notification styling. |
| `~/.config/glimpse/themes/lock.css` | Lock screen styling. |

The default theme name is `adwaita`.

## Choose A Shell Theme

In `~/.config/glimpse/config.toml`:

```toml
theme = "adwaita"
theme_mode = "auto"
```

| Option | Default | Values | Meaning |
|---|---|---|---|
| `theme` | `"adwaita"` | theme name | Selects the shell theme. |
| `theme_mode` | `"auto"` | `auto`, `dark`, `light` | Chooses light/dark styling. |

If `theme = "mytheme"`, Glimpse looks for:

```txt
~/.config/glimpse/themes/mytheme.css
```

If that file does not exist, Glimpse keeps the embedded base theme and logs that the user theme was not found.

## Per-Panel Mode

Panels can also choose a mode:

```toml
[[panels]]
position = "top"
theme_mode = "dark"
left = ["pager"]
center = ["clock"]
right = ["network", "battery", "session"]
```

| Option | Default | Values | Meaning |
|---|---|---|---|
| `theme_mode` | `"dark"` | `auto`, `dark`, `light` | Applies mode classes to that panel. |

Use this when one panel sits on a bright wallpaper area and another sits on a dark one.

## Create A Theme

Create a file:

```txt
~/.config/glimpse/themes/mytheme.css
```

Use it:

```toml
theme = "mytheme"
theme_mode = "dark"
```

Restart the shell after changing the theme name:

```sh
systemctl --user restart glimpse-shell.service
```

CSS changes inside the themes directory are watched while the shell is running.

## Useful Shell Selectors

Start small. These selectors cover the parts most users want to adjust first:

```css
.panel {
  background: rgba(20, 20, 20, 0.82);
  color: #f4f4f4;
}

.applet {
  padding: 0 8px;
}

.applet:hover {
  background: rgba(255, 255, 255, 0.08);
}

.popover,
.card-surface {
  background: rgba(28, 28, 28, 0.96);
  border: 1px solid rgba(255, 255, 255, 0.10);
}

.badge,
.status-dot {
  background: #7aa2f7;
}
```

## Lock Screen Theme

Export starter CSS:

```sh
glimpse-lock --export-css
```

This writes:

```txt
~/.config/glimpse/themes/lock.css
```

Lock config defaults to that path:

```toml
[lock]
css_path = "themes/lock.css"
```

| Option | Default | Meaning |
|---|---|---|
| `css_path` | `"themes/lock.css"` | CSS file loaded over the built-in lock screen style. |

Use preview mode while editing:

```sh
glimpse-lock --preview
```

Preview reloads CSS changes and does not lock your real session.

## Useful Lock Selectors

```css
.lock-clock {
  font-size: 80px;
  font-weight: 600;
}

.lock-date {
  font-size: 18px;
}

.lock-auth-panel {
  margin-top: 180px;
}

.lock-entry {
  background: rgba(255, 255, 255, 0.20);
  color: white;
}

.lock-controls {
  color: white;
}
```

## Practical Style Rules

| Rule | Why |
|---|---|
| Keep contrast high | Panel and lock text must stay readable on busy wallpapers. |
| Use transparency carefully | A little glass effect looks polished; too much makes text noisy. |
| Pick one accent color | Status badges and warnings should not fight each other. |
| Keep animations subtle | The desktop should feel smooth, not distracting. |
| Avoid giant panel labels | Long labels crowd the panel and break screenshots. |
| Test on your real wallpaper | A theme that looks good on a flat color can fail on an image. |

## Troubleshooting

| Problem | Check |
|---|---|
| Theme did not change | Confirm `theme = "name"` matches `~/.config/glimpse/themes/name.css`. |
| CSS edits do not reload | Make sure the file is inside `~/.config/glimpse/themes/`. |
| Lock CSS did not load | Run `glimpse-lock --export-css`, then check `css_path`. |
| Text is hard to read | Increase background opacity or lower wallpaper brightness. |
