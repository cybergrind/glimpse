use glimpse::mpris::protocol::MprisPlayer;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    factory::FactoryVecDeque,
    gtk::{self, prelude::*},
};

use super::components::player_row_factory::{MprisPlayerRowItem, MprisPlayerRowItemInit};

pub struct MprisPopover {
    popover: gtk::Popover,
    empty_label: gtk::Label,
    max_rows: usize,
    show_artwork: bool,
    rows: FactoryVecDeque<MprisPlayerRowItem>,
}

pub struct MprisPopoverInit {
    pub parent: gtk::Box,
    pub max_rows: usize,
    pub show_artwork: bool,
}

#[derive(Debug, Clone)]
pub enum MprisPopoverInput {
    Toggle,
    UpdatePlayers(Vec<MprisPlayer>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisPopoverOutput {
    Previous { player_id: String },
    PlayPause { player_id: String },
    Next { player_id: String },
    Raise { player_id: String },
}

fn visible_players(players: Vec<MprisPlayer>, max_rows: usize) -> Vec<MprisPlayer> {
    players.into_iter().take(max_rows).collect()
}

#[relm4::component(pub)]
impl SimpleComponent for MprisPopover {
    type Init = MprisPopoverInit;
    type Input = MprisPopoverInput;
    type Output = MprisPopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "mpris-popover",

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,

                #[name(empty_label)]
                gtk::Label {
                    set_label: "No media players",
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                },

                gtk::ScrolledWindow {
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_propagate_natural_height: true,

                    #[local_ref]
                    rows_box -> gtk::Box {}
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let rows = FactoryVecDeque::builder()
            .launch(rows_box.clone())
            .forward(sender.output_sender(), |output| output);

        let widgets = view_output!();

        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let model = MprisPopover {
            popover: widgets.root.clone(),
            empty_label: widgets.empty_label.clone(),
            max_rows: init.max_rows,
            show_artwork: init.show_artwork,
            rows,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            MprisPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            MprisPopoverInput::UpdatePlayers(players) => {
                self.sync_rows(players);
            }
        }
    }
}

impl MprisPopover {
    fn sync_rows(&mut self, players: Vec<MprisPlayer>) {
        let players = visible_players(players, self.max_rows);
        self.empty_label.set_visible(players.is_empty());

        let mut guard = self.rows.guard();
        guard.clear();
        for player in players {
            guard.push_back(MprisPlayerRowItemInit {
                player,
                show_artwork: self.show_artwork,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use glimpse::mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer};

    use super::visible_players;

    fn test_player_with_id(player_id: &str) -> MprisPlayer {
        MprisPlayer {
            player_id: player_id.into(),
            bus_name: format!("org.mpris.MediaPlayer2.{player_id}"),
            identity: player_id.into(),
            playback_status: MprisPlaybackStatus::Playing,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            panel_label: String::new(),
            subtitle: String::new(),
            artwork: MprisArtwork::None,
            position: None,
            length: None,
            progress_visible: false,
            can_play_pause: true,
            can_go_previous: true,
            can_go_next: true,
            can_raise: true,
            last_active: 1,
        }
    }

    #[test]
    fn visible_players_respects_max_rows() {
        let players = vec![
            test_player_with_id("one"),
            test_player_with_id("two"),
            test_player_with_id("three"),
        ];

        let ids: Vec<String> = visible_players(players, 2)
            .into_iter()
            .map(|player| player.player_id)
            .collect();

        assert_eq!(ids, vec!["one", "two"]);
    }
}
