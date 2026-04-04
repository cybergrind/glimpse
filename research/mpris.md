# MPRIS Provider

**Source:** MPRIS2 D-Bus (`org.mpris.MediaPlayer2.*`, session bus)

**What it does:** Discovers media players, reports playback state and track metadata, provides playback controls (play/pause/next/prev/seek/volume), and tracks player appearance/disappearance.

## System Interface

### Player discovery

Bus name pattern: `org.mpris.MediaPlayer2.{player}` (e.g. `org.mpris.MediaPlayer2.firefox`, `org.mpris.MediaPlayer2.spotify`)

Multiple instances: `org.mpris.MediaPlayer2.{player}.instance{N}`

All players expose object path: `/org/mpris/MediaPlayer2`

Discovery:
- `org.freedesktop.DBus.ListNames()` → filter names starting with `org.mpris.MediaPlayer2.`
- Watch `NameOwnerChanged` signal with `arg0namespace='org.mpris.MediaPlayer2'` for live player add/remove

### org.mpris.MediaPlayer2 (root interface)

Methods:
- `Raise()` — bring player UI to front
- `Quit()` — stop the player

Properties:
- `Identity: String` (RO) — human-readable player name (e.g. "Spotify", "Firefox")
- `DesktopEntry: String` (RO) — .desktop file basename without extension
- `CanQuit: bool` (RO)
- `CanRaise: bool` (RO)
- `HasTrackList: bool` (RO)
- `SupportedUriSchemes: Vec<String>` (RO)
- `SupportedMimeTypes: Vec<String>` (RO)

### org.mpris.MediaPlayer2.Player (player interface)

Methods:
- `Play()` — start/resume playback
- `Pause()` — pause playback
- `PlayPause()` — toggle play/pause
- `Stop()` — stop playback
- `Next()` — skip to next track
- `Previous()` — skip to previous track
- `Seek(offset: i64)` — seek by offset in microseconds (negative = backward)
- `SetPosition(track_id: ObjectPath, position: i64)` — set absolute position in microseconds
- `OpenUri(uri: String)` — open URI as current track

Properties (read-only):
- `PlaybackStatus: String` — "Playing", "Paused", or "Stopped"
- `Metadata: HashMap<String, Variant>` — current track metadata (see below)
- `Position: i64` — current position in microseconds
- `CanGoNext: bool`
- `CanGoPrevious: bool`
- `CanPlay: bool`
- `CanPause: bool`
- `CanSeek: bool`
- `CanControl: bool`
- `MinimumRate: f64`
- `MaximumRate: f64`

Properties (read-write):
- `LoopStatus: String` — "None", "Track", or "Playlist"
- `Rate: f64` — playback speed (1.0 = normal, must not be 0.0)
- `Shuffle: bool`
- `Volume: f64` — 0.0 to 1.0+ (can exceed 1.0 for amplification)

Signals:
- `Seeked(position: i64)` — position changed discontinuously

### Metadata dictionary keys

MPRIS keys:
- `mpris:trackid: ObjectPath` — **required**, unique track ID
- `mpris:length: i64` — duration in microseconds
- `mpris:artUrl: String` — artwork URI (file:// or https://)

Xesam keys:
- `xesam:title: String` — track title
- `xesam:artist: Vec<String>` — track artists (array, not single string)
- `xesam:album: String` — album name
- `xesam:albumArtist: Vec<String>` — album artists
- `xesam:url: String` — media location URI
- `xesam:trackNumber: i32` — track number on album
- `xesam:discNumber: i32` — disc number
- `xesam:genre: Vec<String>` — genre(s)
- `xesam:comment: Vec<String>` — freeform comments
- `xesam:composer: Vec<String>` — composer(s)
- `xesam:contentCreated: String` — ISO 8601 date
- `xesam:userRating: f64` — 0.0–1.0

## Topics

- `mpris.players` — list of active media players
- `mpris.player.{id}` — single player state (playback status, track metadata, position, volume)

## Methods

- `mpris.play(player_id: String)` — start/resume playback
- `mpris.pause(player_id: String)` — pause
- `mpris.play_pause(player_id: String)` — toggle
- `mpris.stop(player_id: String)` — stop
- `mpris.next(player_id: String)` — next track
- `mpris.previous(player_id: String)` — previous track
- `mpris.seek(player_id: String, offset_usec: i64)` — seek by offset in microseconds
- `mpris.set_position(player_id: String, position_usec: i64)` — set absolute position
- `mpris.set_volume(player_id: String, volume: f64)` — set volume (0.0–1.0+)
- `mpris.set_loop(player_id: String, status: LoopStatus)` — set loop mode
- `mpris.set_shuffle(player_id: String, shuffle: bool)` — set shuffle
- `mpris.raise(player_id: String)` — bring player window to front

## Types

```rust
/// Playback state
enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// Loop/repeat mode
enum LoopStatus {
    /// Stop at end of tracklist
    None,
    /// Repeat current track
    Track,
    /// Repeat entire tracklist
    Playlist,
}

/// Track metadata
struct TrackMetadata {
    /// Unique track identifier
    track_id: String,
    title: Option<String>,
    /// Track artists (may be multiple)
    artists: Vec<String>,
    album: Option<String>,
    album_artists: Vec<String>,
    /// Artwork URI (file:// or https://)
    art_url: Option<String>,
    /// Duration in microseconds
    length: Option<i64>,
    track_number: Option<i32>,
    disc_number: Option<i32>,
    genres: Vec<String>,
    /// Media location URI
    url: Option<String>,
}

/// A media player instance
struct MprisPlayer {
    /// Bus name suffix (e.g. "spotify", "firefox.instance12345")
    id: String,
    /// Human-readable name (e.g. "Spotify")
    identity: String,
    /// .desktop file basename
    desktop_entry: Option<String>,
    playback_status: PlaybackStatus,
    metadata: TrackMetadata,
    /// Current position in microseconds
    position: i64,
    /// Volume 0.0–1.0+
    volume: f64,
    loop_status: LoopStatus,
    shuffle: bool,
    /// Playback rate (1.0 = normal)
    rate: f64,
    can_go_next: bool,
    can_go_previous: bool,
    can_play: bool,
    can_pause: bool,
    can_seek: bool,
    can_control: bool,
}
```

## Icons

Playback controls:
- `media-playback-start-symbolic` — play
- `media-playback-pause-symbolic` — pause
- `media-playback-stop-symbolic` — stop
- `media-skip-forward-symbolic` — next track
- `media-skip-backward-symbolic` — previous track
- `media-seek-forward-symbolic` — seek forward
- `media-seek-backward-symbolic` — seek backward

Status:
- `media-playlist-repeat-symbolic` — loop/repeat
- `media-playlist-shuffle-symbolic` — shuffle

All icons above are available in Adwaita icon theme.

## Crates

- `mpris` — MPRIS2 Rust bindings with `PlayerFinder`, `Player`, `Metadata`, event streams, progress tracking. Recommended.
- `zbus` (5) — alternative: raw D-Bus access for MPRIS

## Change Detection

**Player discovery:** `NameOwnerChanged` D-Bus signal with `arg0namespace='org.mpris.MediaPlayer2'`. Fires when players appear or disappear. Fully reactive.

**Player state:** `PropertiesChanged` signal on `org.mpris.MediaPlayer2.Player` interface. Fires on playback status, metadata, volume, loop, shuffle changes.

**Position:** `Seeked(position: i64)` signal fires on discontinuous position changes (user seek, track change). For continuous position tracking, poll `Position` property or use `mpris` crate's `ProgressTracker`.

**Note:** `CanControl` property does NOT emit `PropertiesChanged` when it changes — must be polled if needed.

## Features

- Discover all active media players on session bus
- Track metadata: title, artist, album, artwork URL, duration, track/disc number, genres
- Playback controls: play, pause, stop, next, previous, seek
- Volume control per player
- Loop mode (none/track/playlist) and shuffle toggle
- Playback rate control
- Position tracking with seek support
- Player identity and .desktop file association
- Multiple simultaneous player support
- Player appearance/disappearance detection
- Album art URL resolution
- Capability queries (can play, can seek, etc.)

## Notes

- `xesam:artist` is `Vec<String>`, not a single string — always handle as array
- Position is in microseconds (not milliseconds)
- Volume can exceed 1.0 for over-amplification
- `mpris:artUrl` can be `file://` (local) or `https://` (remote) — clients may need to fetch
- Some players (e.g. browsers) create ephemeral instances with random suffixes
- Not all players implement all methods — check `CanPlay`, `CanSeek`, etc. before calling
- `DesktopEntry` can be used to look up the app icon via freedesktop icon theme
