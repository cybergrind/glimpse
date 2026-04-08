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

# Run the daemon
RUST_LOG=info cargo run -p glimpsed

# Run the panel (from glimpse-panel directory for CSS loading)
cd glimpse-panel && RUST_LOG=info cargo run


## Architecture

Glimpse is a Wayland status panel ecosystem with a client-server architecture.
It has shared code in `glimpse` crate and panel code in `glimpse-panel`.

`glimpsed` is a legacy and should not be read as a reference.

### Configuration

Config is loaded from (in priority order):
1. `GLIMPSE_PANEL_CONFIG` env var
2. `./panel.toml`
3. `$XDG_CONFIG_HOME/glimpse/panel.toml`

Applets are configured in `[[panels]]` sections with per-applet `[applets.name]` overrides.

### Dev vs Prod Features

The `dev` feature (enabled by default) controls:
- Base CSS loaded from file vs embedded
- File watching for base CSS hot-reload

## Provider Development

## Applet Development

- decompose popover into subcomponents
- use zbus macro to create proxies
- do not hardocde styles in rust, put then in theme.css
- if applet has popover, it has to have "hoverable" class
- try to keep logic in the applet and keep popover only for ui whenever possible
- when you create a new provider, use enums, structs and methods. use free functions only if they are truly standalone
- zbus::Connection is Arc, you can clone it into provider's new()
### Key patterns

- Persistent widget maps (`HashMap<key, WidgetRow>`) instead of clear+rebuild — preserves context menus, scroll position
- Hero pattern: 32px icon + title + subtitle, consistent across all popover applets
- Box spacing is a widget property, not CSS — keep spacing values in Rust code
- PopoverMenu for right-click: GIO SimpleActionGroup + PopoverMenu from model

### CSS conventions

- All styles in `theme.css`, not hardcoded in Rust
- CSS variables: `--popover-padding`, `--popover-section-spacing`, `--dim-opacity`, `--accent-bg`
- Popover structure: `.foo-popover contents > box { margin: var(--popover-padding) }`
- Button text: always `font-weight: normal` on both button and label
- Tabular numbers: `font-variant-numeric: tabular-nums` for all numeric displays

# Important rules
- always use your built-in tools to read, search, grep, write files. Only use bash as the last resort
- if i don't allow you a certain actions - do not workaround it.
- do not write any code unless i ask you - print instead. I want to review, i want to type everything by hand
- make sure you don't hardcode styles in rust code, use theme.css for that

## GTK4 Widget Layout Pitfalls

When creating panel widgets, avoid these common GTK4 layout issues:

- **Widgets stretch to fill panel height** — add `set_valign(gtk::Align::Center)` on indicator/dot widgets
- **Content not centered in fixed-width containers** — don't use `min-width` in CSS for centering. Instead use `padding: 0 Npx` so the box wraps content with equal padding. Or use `label.set_xalign(0.5)` for text centering.
- **`set_hexpand(true)` causes widgets to fill available space** — never use hexpand on panel applet children. The panel is a horizontal box; hexpand makes one applet consume all free space.
- **`set_halign(Center)` on a box inside a horizontal parent** — this centers the box itself but doesn't center its children. Center the child (label), not the container.
- **CSS `min-width` creates dead space** — the label sits at the start of the box. Use padding instead, or `set_size_request` with `label.set_xalign(0.5)`.


<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->

Use 'bd' for task tracking

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd dolt push
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->

d
