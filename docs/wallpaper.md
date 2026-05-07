# Wallpaper

Wallpaper sets the desktop background. It can show a solid color, an image, and a blurred backdrop that helps panels and lock screens feel softer.

## Export Starter Config

```sh
glimpse-wallpaper --export-config
```

The usual file is:

```txt
~/.config/glimpse/wallpaper.toml
```

## Solid Color

```toml
[wallpaper]
color = "#101010"
fit = "cover"
transition_ms = 800

[backdrop]
enabled = true
blur_radius = 24
```

This is enough for a clean dark background.

## Image Wallpaper

```toml
[wallpaper]
color = "#101010"
path = "/home/alex/Pictures/wallpapers/coast.jpg"
fit = "cover"
transition_ms = 800

[backdrop]
enabled = true
blur_radius = 24
```

Supported image formats include common wallpaper formats such as JPG, PNG, WebP, and HEIF/HEIC.

## Fit Modes

| Mode | Result |
|---|---|
| `cover` | Fill the screen and crop edges if needed. Best default. |
| `contain` | Show the whole image with empty space if aspect ratios differ. |
| `fill` | Stretch the image to the screen. |

## Backdrop

The backdrop is a blurred background layer. It is enabled by default and uses the wallpaper image unless you give it a separate image.

```toml
[backdrop]
enabled = true
path = "/home/alex/Pictures/wallpapers/backdrop.jpg"
blur_radius = 24
```

Use a separate backdrop if you want the lock screen and shell effects to feel calmer than the main wallpaper.

## Reloading

Glimpse watches the wallpaper config and image files. When you change the file, the wallpaper updates without restarting the service.

If a new image cannot be loaded, the previous image stays visible when possible. If no image is available, Glimpse falls back to the configured color.

## Tips For Pretty Setups

| Goal | Tip |
|---|---|
| Clean screenshot | Use `cover`, a high-resolution image, and a dark `color` fallback. |
| Frosted lock screen | Enable backdrop and set `blur_radius = 24` or higher. |
| Faster startup | Reuse the same wallpaper path instead of constantly renaming files. |
| Multi-monitor | Use images that still look good when cropped differently per screen. |
