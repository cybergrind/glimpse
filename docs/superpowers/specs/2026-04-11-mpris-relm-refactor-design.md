# MPRIS Relm Refactor Design

## Goal

Refactor the existing MPRIS panel applet to use a real Relm4 component tree and a factory-backed player list, while preserving the current user-visible behavior, controls, and styling.

## Scope

This design covers:

- refactoring the existing MPRIS applet UI in `glimpse-panel`
- converting the popover shell to declarative `view!`
- replacing manual player row widget building and manual `HashMap` row bookkeeping with Relm4 components and a factory
- keeping the current applet-to-service command boundary intact
- preserving artwork loading, transport controls, progress display, and popover behavior

This design does not cover:

- MPRIS provider/service protocol changes
- queue browsing
- seek slider interaction changes
- new controls or settings
- visual redesign of the MPRIS card

## Existing Problems

The current MPRIS applet uses the right top-level service boundary but the popover internals are still too manual:

- `mpris/applet.rs` is already mostly declarative and structurally fine
- `mpris/popover.rs` hand-builds player card widgets with GTK constructors
- repeated player rows are managed through a manual `HashMap<String, PlayerRowWidgets>`
- row updates mix widget lookup, tree mutation, and data mapping in one file
- the component tree is harder to reason about than newer panel work such as notifications

This makes the popover harder to extend, harder to test, and inconsistent with the preferred Relm4 approach already established in the repo.

## User Experience

User-facing behavior stays the same.

### Panel

The panel continues to show:

- one compact now-playing label
- the current tooltip behavior
- the same click interaction for opening and closing the popover
- the same hide-when-empty behavior

### Popover

The popover continues to show:

- a vertically stacked list of player cards
- optional artwork
- title, subtitle, controls, and progress
- empty state when there are no visible players
- the same player ordering and `max_rows` capping

This refactor is structural, not a redesign.

## Architecture

### Top-level applet

Keep `mpris/applet.rs` as the app-facing orchestrator.

Responsibilities remain:

- subscribe to `MprisServiceHandle`
- derive panel label, tooltip, and hidden state from service state
- own the child popover controller
- translate popover outputs into `MprisServiceCommand`

This matches the same high-level pattern used by Bluetooth and Network.

### Popover

Refactor `mpris/popover.rs` into a declarative Relm4 shell.

Responsibilities:

- own the GTK `Popover`
- attach it to the applet root in `init()`
- declare the empty-state label and the factory mount point in `view!`
- receive player snapshots and update a factory-backed list
- emit typed row actions upward

The popover should no longer build player cards manually and should no longer own a `HashMap<String, PlayerRowWidgets>`.

### Player row component

Create a dedicated Relm4 player-row component under `glimpse-panel/src/applets/mpris/components/`.

Responsibilities:

- render one player card via `view!`
- own title, subtitle, controls, artwork slot, and progress slot
- emit typed intents upward:
  - previous
  - play/pause
  - next
  - raise
- update UI from a single `MprisPlayer` input model

This component is the main replacement for the current manual `build_row()` / `update_row()` helpers.

### Factory list

Use a Relm4 factory for repeated player rows.

Responsibilities:

- represent visible players in display order
- keep stable item identity by `player_id`
- update rows from incoming service state without a custom row map
- keep row creation, removal, and reordering inside Relm’s collection model

This is the main architectural difference from the current MPRIS popover and the main place where this refactor should leverage Relm more than the older applets currently do.

## Component Boundaries

Expected panel file structure after the refactor:

- `glimpse-panel/src/applets/mpris/applet.rs`
  - applet orchestrator
- `glimpse-panel/src/applets/mpris/popover.rs`
  - popover shell + factory ownership
- `glimpse-panel/src/applets/mpris/components/mod.rs`
  - component exports
- `glimpse-panel/src/applets/mpris/components/player_row.rs`
  - single player card component
- `glimpse-panel/src/applets/mpris/components/player_row_factory.rs`
  - factory item type for repeated rows

The exact factory file naming can be adjusted, but the responsibilities should remain split this way.

## Data Flow

### Service to applet

`MprisServiceState` continues to flow into the applet.

The applet continues to:

- compute the compact panel label
- compute tooltip and hidden state
- pass the player list into the popover

### Applet to popover

The popover receives the visible player vector as typed input.

The popover then:

- applies `max_rows`
- syncs the factory list to those players
- shows or hides the empty state

### Row to applet commands

Row button clicks do not call the service directly.

Flow remains:

- row emits typed output
- popover forwards typed output
- applet translates to `MprisServiceCommand`
- applet sends the command through `MprisServiceHandle`

This preserves the repo’s applet/popover UI boundary rule.

## Imperative Exceptions

The refactor should maximize Relm, but a small amount of imperative code remains valid.

Allowed imperative work:

- attaching the popover to its parent widget in `init()`
- artwork texture loading from file or network
- GTK APIs that do not have a practical declarative equivalent
- service subscription and async command sending in the applet

Not allowed after the refactor:

- hand-built GTK card trees for player rows
- manual persistent row maps in the popover
- callback closures that directly perform provider/service work

## Artwork Handling

Artwork behavior remains the same.

Requirements:

- player row always has an artwork slot in the template
- slot is hidden when artwork is disabled or unavailable
- file paths and file URIs load from disk
- remote URLs continue to load asynchronously
- failures fall back to the current symbolic placeholder
- reload only when the artwork source actually changes

The side effect of loading textures may remain imperative inside the row component.

## Progress Handling

Progress behavior remains the same.

Requirements:

- show the progress row only when both position and length exist and progress is marked visible
- keep the existing duration formatting
- keep the existing fraction calculation and clamping behavior

These updates can remain simple widget refresh logic in the row component.

## Testing

### Keep existing pure tests

Preserve or move the current tests for:

- artwork source typing
- artwork reload change detection
- progress visibility rules
- panel label formatting

### Add focused row/component tests

Add tests where practical for:

- visible player truncation by `max_rows`
- row output mapping for button actions
- row subtitle fallback visibility
- progress row visibility state

### Verification

Run at minimum:

- `cargo check -p glimpse-panel`
- targeted MPRIS tests in `glimpse-panel`

## Migration Strategy

Do the refactor in-place in the existing MPRIS applet.

Recommended order:

1. add new MPRIS component module structure
2. move one player card into a dedicated component
3. convert the popover shell to declarative `view!`
4. replace the manual row map with a factory-backed list
5. remove obsolete row builder/update helpers
6. verify that behavior and styling remain stable

## Success Criteria

The refactor is complete when:

- the MPRIS applet still behaves the same from the user’s perspective
- the popover no longer hand-builds player card widget trees
- repeated player rows no longer use a custom `HashMap` widget map
- the repeated player list uses a Relm4 factory
- row controls still emit typed outputs upward instead of calling the service directly
- artwork, controls, and progress still work as before
