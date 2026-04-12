# Clock Applet Relm Structure Refactor

## Goal

Refactor the clock applet so it follows the same component ownership pattern as the newer applets in the panel:

- the applet owns durable state and service coordination
- the popover is a UI composition shell
- leaf components render state and emit narrow intents upward
- repeated UI is managed through Relm factories instead of hand-managed widget lists

This refactor is structural. It should preserve current clock behavior while making the component boundaries consistent with audio, battery, and bluetooth.

## Current Problems

The clock applet already owns the calendar service handle and the top-level timer, but the current subtree still has older responsibilities spread across the popover and some leaf widgets.

Current issues:

- `clock/popover.rs` owns `selected_date`, `follow_today`, and the latest `CalendarServiceState`
- `clock/popover.rs` resolves month and day snapshots from cached service state instead of receiving already-resolved UI state from the applet
- `clock/popover.rs` constructs service commands in response to child output, which makes the popover a coordinator instead of a UI shell
- `clock/events.rs` manages rows imperatively with stored widget handles instead of a Relm repeated-list abstraction
- `clock/world.rs` manually appends rows during initialization instead of rendering from state through a declarative component structure

These boundaries make clock unlike the rest of the applets that were already cleaned up.

## Target Architecture

### Applet Ownership

`Clock` in `glimpse-panel/src/applets/clock/applet.rs` becomes the owner of all durable applet state:

- current formatted panel label
- latest `CalendarServiceState`
- `selected_date`
- `follow_today`

The applet remains responsible for:

- subscribing to the calendar service
- sending `CalendarServiceCommand`s
- driving the local clock tick
- deciding when hidden popover children should or should not receive updates
- resolving visible month/day data from cached service state

### Popover Ownership

`Popover` in `glimpse-panel/src/applets/clock/popover.rs` becomes a composition component only.

Responsibilities:

- build the declarative layout with `view!`
- host the `Date`, `Calendar`, `WorldClock`, and `Events` child components
- forward child outputs upward as typed `PopoverOutput` messages
- accept already-resolved UI state from the applet and fan it out to the relevant children

Responsibilities removed from the popover:

- owning the selected date
- deciding whether the applet follows today
- caching service state
- resolving month/day snapshots from service caches
- constructing calendar service commands

## Component Contracts

### Clock Applet

`ClockInput` remains the top-level entry point for ticks, service-state updates, popover toggles, and child outputs.

The applet will add helper methods to:

- sync the popover from the latest applet-owned state
- resolve the selected day snapshot from `day_cache`, `month_cache`, or `today`
- update applet-owned selection state when the user picks a day
- decide whether a selection change should request a day refresh

`Clock` remains the only component that sends `CalendarServiceCommand`s.

### Popover

`PopoverInput` will be reduced to UI-facing updates such as:

- toggle visibility
- tick visible children
- set selected date
- set month snapshot or clear month
- set selected day snapshot and refresh flag

`PopoverOutput` will expose only child intents:

- selected date changed
- month load requested
- day load requested

The popover will not inspect `CalendarServiceState`.

### Calendar

`Calendar` already uses a factory-based grid and can remain mostly unchanged.

It continues to emit:

- `SelectedDate(NaiveDate)`
- `LoadMonth { year, month }`

### Date

`Date` is already a simple state-driven component and remains unchanged except for any minor message-shape adjustments needed by the new popover input flow.

### Events

`Events` will be rewritten as a proper Relm component with durable model state and a factory-backed repeated event list.

Responsibilities:

- render a selected-day event list from applet-provided data
- render an empty state when there are no events
- request a day load only when instructed by the parent refresh flow

Responsibilities removed:

- manual widget-handle storage for the list container and empty-state label
- imperative clear-and-rebuild row management

### World Clock

`WorldClock` will be rewritten to render timezone entries from model state instead of manual append logic in `init`.

Because the timezone list is configured and relatively small, it can use either:

- a factory if that matches the final row design cleanly, or
- declarative child composition with stable controllers if the row count is fixed by configuration and does not change at runtime

The chosen approach must still avoid one-off manual box assembly that hides the component structure.

## Data Flow

### Service Updates

1. `Clock` receives a `CalendarServiceState` update from the subscribed service handle.
2. `Clock` stores the new state.
3. `Clock` resolves the currently visible month and selected-day data using applet-owned `selected_date`.
4. If the popover is visible, `Clock` sends the resolved UI state into `Popover`.
5. `Popover` forwards that state to `Date`, `Calendar`, `Events`, and `WorldClock`.

If the popover is hidden, the applet stores the new state but does not fan updates into popover children. On the next open, the applet performs a full sync.

### User Selection

1. `Calendar` emits `SelectedDate(date)`.
2. `Popover` forwards that as a typed `PopoverOutput`.
3. `Clock` updates `selected_date` and recomputes `follow_today`.
4. `Clock` resolves the selected-day snapshot from cached state.
5. `Clock` updates `Date`, `Calendar`, and `Events` through the popover inputs.
6. If the selected day is not cached and requires a fetch, `Clock` sends `CalendarServiceCommand::LoadDay`.

### Month Changes

1. `Calendar` emits `LoadMonth { year, month }`.
2. `Popover` forwards the request upward.
3. `Clock` sends `CalendarServiceCommand::LoadMonth { year, month }`.

### Tick Handling

1. The applet timer updates the panel label every second.
2. Visible popover children receive tick input only while the popover is shown.
3. If `follow_today` is enabled and the local date changed, the applet moves `selected_date` to today and re-syncs the popover.

## Error Handling

If the calendar service becomes unavailable:

- the applet stores `CalendarServiceState::default()`
- the popover is cleared through the normal resolved-state sync path
- the applet continues showing the local time label because that does not depend on the service

If a command send fails:

- the applet logs a warning
- no retry loop is added in this refactor

## Testing

The refactor should add or preserve tests at the pure-logic boundary rather than trying to over-test GTK internals.

Required coverage:

- selected-day resolution uses `day_cache` first, then `month_cache`, then `today`
- missing selected-day data produces the correct refresh decision
- hidden popover state does not require tick forwarding
- any new pure helper introduced for clock state synchronization has unit coverage

Verification gate:

- `cargo check -p glimpse-panel`

Optional follow-up tests can be added for event/world row shaping if the new helpers make that practical without fragile widget assertions.

## Scope Boundaries

Included:

- `glimpse-panel/src/applets/clock/applet.rs`
- `glimpse-panel/src/applets/clock/popover.rs`
- `glimpse-panel/src/applets/clock/events.rs`
- `glimpse-panel/src/applets/clock/world.rs`
- any small supporting row/factory modules needed for `Events` or `WorldClock`

Excluded:

- new calendar-service features
- changes to the visual design of the clock popover
- unrelated cleanup in other applets
- protocol changes in `glimpse/src/calendar`

## Success Criteria

The refactor is complete when:

- `Clock` owns the durable clock state and all calendar-service commands
- `Popover` is reduced to layout, state fan-out, and child forwarding
- `Events` no longer uses imperative clear-and-rebuild list wiring
- `WorldClock` no longer relies on ad hoc manual child assembly
- current user-visible clock behavior remains intact
- `cargo check -p glimpse-panel` passes
