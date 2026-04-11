use glimpse::mpris::protocol::MprisPlayer;

#[derive(Debug, Clone)]
pub struct MprisPlayerRowItem {
    pub player: MprisPlayer,
    pub show_artwork: bool,
}
