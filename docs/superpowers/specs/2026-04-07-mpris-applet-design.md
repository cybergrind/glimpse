# MPRIS Applet Design

## Goal

Add an MPRIS-backed panel applet that shows a single compact now-playing label in the panel and a flat multi-player popover with per-player media controls.

## Scope

This design covers:

- a new `mpris` provider in `glimpsed`
- a new `mpris` applet in `glimpse-panel`
- provider topics and methods
- panel selection and popover behavior
- testing and failure handling boundaries

This design does not include:

- queue browsing
- lyrics
- volume control
- player-specific settings UI
- seek slider or live position scrubbing

## User Experience

### Panel

The panel shows exactly one item:

```text
[ artist - track ]
```

Rules:

- Show the most recently active player only.
- If multiple players exist, do not render multiple panel items.
- Default format is `"{artist} - {track}"`.
- If `artist` is missing, fall back to `track`.
- If `track` is missing, fall back to player identity.
- If no players exist, hide the applet by default.

### Popover

The popover shows a flat list of equal rows:

```text
[img] [track................] [prev] [play/pause] [next]
      [artist]
```

Rules:

- One row per player.
- No expanded hero row.
- All rows use the same layout and importance.
- Sort rows by recency, newest first.
- Track is the first line.
- Artist is the second line.
- If artist is missing, fall back to album, then player identity.
- Artwork is optional. If unavailable, show a symbolic media icon.
- Previous and next buttons disable when unsupported.
- Play/pause reflects the row player's current playback status.

## Architecture

Use the existing daemon-backed applet architecture.

### Provider responsibilities

The `mpris` provider in `glimpsed` is responsible for:

- discovering players exposed as `org.mpris.MediaPlayer2.*`
- tracking player appearance and disappearance
- creating and refreshing D-Bus proxies
- normalizing metadata into a stable applet-facing shape
- computing `last_active` for recency ordering
- publishing a current-player snapshot and the full player list
- accepting row-specific control calls

### Applet responsibilities

The `mpris` applet in `glimpse-panel` is responsible for:

- subscribing to provider topics
- rendering the compact panel label
- rendering the flat multi-player popover
- dispatching controls for the clicked row
- loading artwork asynchronously in the UI layer
- falling back cleanly when artwork or metadata is missing

This keeps D-Bus integration and selection logic out of GTK and follows the same provider/applet split already used elsewhere in the project.

## Provider Interface

### Topics

```text
mpris.current
mpris.players
```

### Methods

```text
mpris.play_pause
mpris.previous
mpris.next
mpris.raise
```

### Topic payloads

`mpris.current` contains the selected panel player:

```text
- player_id
- bus_name
- identity
- artist
- track
- album
- status
- art_url
- last_active
```

`mpris.players` contains all visible players:

```text
- player_id
- bus_name
- identity
- artist
- track
- album
- status
- art_url
- can_go_previous
- can_play_pause
- can_go_next
- last_active
```

`player_id` should be stable for the lifetime of a player instance and suitable for row-targeted commands. `bus_name` should be retained for diagnostics and D-Bus routing.

## Recency Model

The provider selects the panel player by `last_active`.

`last_active` updates when:

- playback status changes
- track metadata changes
- the user sends a control to that player

`last_active` does not update from position ticks. This avoids constant list reordering and panel churn.

Selection rules:

- `mpris.current` is the player with the newest `last_active`.
- If nothing is playing but paused players exist, keep the most recent paused player as current.
- If the current player disappears, recompute from the remaining players.
- If no players remain, publish an empty player list and no current player snapshot.

## Control Flow

Per-row controls target only that row's player.

```text
row button click
  -> applet calls mpris.previous/play_pause/next { player_id }
  -> provider routes the D-Bus call to that player
  -> provider refreshes cached state
  -> provider republishes mpris.players and mpris.current
```

`mpris.raise` is optional in the first UI pass, but the provider interface should reserve it so the applet can add a row action later without changing provider naming.

## Failure Handling

- If the provider cannot subscribe to MPRIS, the applet should fail quietly and remain hidden.
- A command failure for one player must not remove or invalidate other players.
- Invalid or unreachable artwork URLs must not break row rendering.
- Missing metadata should degrade through explicit fallbacks rather than empty labels.
- Players that do not support previous or next should still render with disabled controls rather than disappearing controls.

## File Boundaries

### Daemon

Expected files:

```text
glimpsed/src/providers/mpris.rs
glimpsed/src/providers/mod.rs
glimpsed/src/main.rs
```

### Panel

Expected files:

```text
glimpse-panel/src/applets/mpris/mod.rs
glimpse-panel/src/applets/mpris/config.rs
glimpse-panel/src/applets/mpris/applet.rs
glimpse-panel/src/applets/mpris/popover.rs
glimpse-panel/src/applets/mod.rs
glimpse-panel/src/theme.css
```

## Configuration

Start with a minimal applet config surface:

```toml
[mpris]
label_format = "{artist} - {track}"
show_artwork = true
hide_when_empty = true
max_rows = 6
```

Notes:

- `hide_when_empty = true` should be the default.
- `max_rows` limits visible popover rows only; the provider should still track all players.

## Testing

### Provider tests

Add unit tests for:

- metadata normalization from raw MPRIS values
- recency ordering and current-player selection
- fallback label selection when artist or track is missing
- removal of vanished players
- command targeting for the correct player id

### Applet tests

Add focused tests where practical for:

- panel label formatting
- popover row state mapping
- button enabled or disabled behavior from capability flags

### Manual verification

Verify these scenarios:

- one active player
- two simultaneous players
- paused plus playing combination
- missing artwork
- missing artist and missing track metadata
- player disappears while the popover is open

## Open Implementation Notes

- Artwork loading belongs in the applet, not the provider.
- The popover should use GTK layout patterns already established in the repo, including centered vertical alignment and no `hexpand` on panel children.
- The provider should prefer stable, explicit normalization over mirroring raw MPRIS metadata directly into UI payloads.
