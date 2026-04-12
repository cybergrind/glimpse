use std::collections::HashMap;

use zbus::zvariant::OwnedValue;

pub const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
pub const MPRIS_ROOT_INTERFACE: &str = "org.mpris.MediaPlayer2";
pub const MPRIS_PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
pub const MPRIS_NAME_PREFIX: &str = "org.mpris.MediaPlayer2.";

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2",
    default_path = "/org/mpris/MediaPlayer2",
    assume_defaults = true
)]
pub trait MprisRoot {
    #[zbus(property)]
    fn identity(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn can_raise(&self) -> zbus::Result<bool>;

    fn raise(&self) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2",
    assume_defaults = true
)]
pub trait MprisPlayer {
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;

    #[zbus(property)]
    fn position(&self) -> zbus::Result<i64>;

    #[zbus(property)]
    fn can_go_previous(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn can_play(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn can_pause(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn can_go_next(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn can_seek(&self) -> zbus::Result<bool>;

    fn play_pause(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpris_constants_match_spec() {
        assert_eq!(MPRIS_PATH, "/org/mpris/MediaPlayer2");
        assert_eq!(MPRIS_ROOT_INTERFACE, "org.mpris.MediaPlayer2");
        assert_eq!(MPRIS_PLAYER_INTERFACE, "org.mpris.MediaPlayer2.Player");
        assert_eq!(MPRIS_NAME_PREFIX, "org.mpris.MediaPlayer2.");

        assert_eq!(
            MPRIS_ROOT_INTERFACE,
            MPRIS_NAME_PREFIX.trim_end_matches('.')
        );
        assert_eq!(
            MPRIS_PLAYER_INTERFACE.strip_prefix(MPRIS_NAME_PREFIX),
            Some("Player")
        );
        assert_eq!(MPRIS_PATH, "/org/mpris/MediaPlayer2");
    }
}
