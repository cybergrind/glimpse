./CLAUDE.md
Use 'bd' for task tracking

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
