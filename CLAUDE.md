# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Development build (default, loads CSS from files)
cargo build

# Production build (embeds CSS in binary)
cargo build --release --no-default-features

# Check compilation
cargo check

# Run the panel (from glimpse-panel directory for CSS loading)
cd glimpse-panel && cargo run
```

## Architecture

Glimpse is a Wayland status panel built with GTK4/libadwaita using the relm4 framework.

### Workspace Structure

- `glimpse-panel/` - Main panel application

### glimpse-panel

A layer-shell panel that anchors to the bottom edge of the screen.

- `src/main.rs` - Entry point, initializes tracing and runs the relm4 app
- `src/app.rs` - Main `App` component using relm4's `SimpleComponent`. Handles:
  - Layer shell setup (bottom-anchored, exclusive zone)
  - CSS provider management (base + theme)
  - File watching for hot-reload of config and CSS
- `src/config.rs` - TOML config loading from multiple paths
- `panel.base.css` - Default styles (embedded in prod, file-loaded in dev)

### Configuration

Config is loaded from (in priority order):
1. `GLIMPSE_PANEL_CONFIG` env var
2. `./panel.toml`
3. `$XDG_CONFIG_HOME/glimpse/panel.toml`

### CSS Theming

Two CSS files are loaded:
- **Base CSS** (`panel.base.css`): Default styles, embedded in release builds
- **Theme CSS** (`panel.theme.css`): User overrides from `./` or `$XDG_CONFIG_HOME/glimpse/`

### Dev vs Prod Features

The `dev` feature (enabled by default) controls:
- Base CSS loaded from file vs embedded
- File watching for base CSS hot-reload
