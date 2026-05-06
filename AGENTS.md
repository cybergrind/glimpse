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

# Run the panel
RUST_LOG=info cargo run -p glimpse --bin glimpse-panel


## Architecture

Glimpse is a Wayland status panel ecosystem with a client-server architecture.
The `glimpse` package contains both the shared core library and the panel binary.

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


<!-- BEGIN BEADS INTEGRATION v:1 profile:full hash:f65d5d33 -->
## Issue Tracking with bd (beads)

**IMPORTANT**: This project uses **bd (beads)** for ALL issue tracking. Do NOT use markdown TODOs, task lists, or other tracking methods.

### Why bd?

- Dependency-aware: Track blockers and relationships between issues
- Git-friendly: Dolt-powered version control with native sync
- Agent-optimized: JSON output, ready work detection, discovered-from links
- Prevents duplicate tracking systems and confusion

### Quick Start

**Check for ready work:**

```bash
bd ready --json
```

**Create new issues:**

```bash
bd create "Issue title" --description="Detailed context" -t bug|feature|task -p 0-4 --json
bd create "Issue title" --description="What this issue is about" -p 1 --deps discovered-from:bd-123 --json
```

**Claim and update:**

```bash
bd update <id> --claim --json
bd update bd-42 --priority 1 --json
```

**Complete work:**

```bash
bd close bd-42 --reason "Completed" --json
```

### Issue Types

- `bug` - Something broken
- `feature` - New functionality
- `task` - Work item (tests, docs, refactoring)
- `epic` - Large feature with subtasks
- `chore` - Maintenance (dependencies, tooling)

### Priorities

- `0` - Critical (security, data loss, broken builds)
- `1` - High (major features, important bugs)
- `2` - Medium (default, nice-to-have)
- `3` - Low (polish, optimization)
- `4` - Backlog (future ideas)

### Workflow for AI Agents

1. **Check ready work**: `bd ready` shows unblocked issues
2. **Claim your task atomically**: `bd update <id> --claim`
3. **Work on it**: Implement, test, document
4. **Discover new work?** Create linked issue:
   - `bd create "Found bug" --description="Details about what was found" -p 1 --deps discovered-from:<parent-id>`
5. **Complete**: `bd close <id> --reason "Done"`

### Quality
- Use `--acceptance` and `--design` fields when creating issues
- Use `--validate` to check description completeness

### Lifecycle
- `bd defer <id>` / `bd supersede <id>` for issue management
- `bd stale` / `bd orphans` / `bd lint` for hygiene
- `bd human <id>` to flag for human decisions
- `bd formula list` / `bd mol pour <name>` for structured workflows

### Auto-Sync

bd automatically syncs via Dolt:

- Each write auto-commits to Dolt history
- Use `bd dolt push`/`bd dolt pull` for remote sync
- No manual export/import needed!

### Important Rules

- ✅ Use bd for ALL task tracking
- ✅ Always use `--json` flag for programmatic use
- ✅ Link discovered work with `discovered-from` dependencies
- ✅ Check `bd ready` before asking "what should I work on?"
- ❌ Do NOT create markdown TODO lists
- ❌ Do NOT use external issue trackers
- ❌ Do NOT duplicate tracking systems

For more details, see README.md and docs/QUICKSTART.md.

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
