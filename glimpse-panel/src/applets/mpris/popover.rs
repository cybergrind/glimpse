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

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowSyncOp {
    Move { from: usize, to: usize },
    Insert { at: usize },
    Remove { at: usize },
}

fn visible_players(players: Vec<MprisPlayer>, max_rows: usize) -> Vec<MprisPlayer> {
    players.into_iter().take(max_rows).collect()
}

fn row_sync_ops(current_ids: &[String], next_ids: &[String]) -> Vec<RowSyncOp> {
    let mut working = current_ids.to_vec();
    let mut ops = Vec::new();

    for (target_index, player_id) in next_ids.iter().enumerate() {
        if working.get(target_index) == Some(player_id) {
            continue;
        }

        if let Some(found_index) = working.iter().position(|id| id == player_id) {
            let moved = working.remove(found_index);
            working.insert(target_index, moved);
            ops.push(RowSyncOp::Move {
                from: found_index,
                to: target_index,
            });
        } else {
            working.insert(target_index, player_id.clone());
            ops.push(RowSyncOp::Insert { at: target_index });
        }
    }

    while working.len() > next_ids.len() {
        working.remove(next_ids.len());
        ops.push(RowSyncOp::Remove { at: next_ids.len() });
    }

    ops
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
        let next_ids = players
            .iter()
            .map(|player| player.player_id.clone())
            .collect::<Vec<_>>();
        let current_ids = guard.iter().map(|row| row.key().to_string()).collect::<Vec<_>>();

        for op in row_sync_ops(&current_ids, &next_ids) {
            match op {
                RowSyncOp::Move { from, to } => guard.move_to(from, to),
                RowSyncOp::Insert { at } => {
                    guard.insert(
                        at,
                        MprisPlayerRowItemInit {
                            player: players[at].clone(),
                            show_artwork: self.show_artwork,
                        },
                    );
                }
                RowSyncOp::Remove { at } => {
                    guard.remove(at);
                }
            }
        }

        for (index, player) in players.into_iter().enumerate() {
            guard[index].sync_player(player);
        }
    }
}

#[cfg(test)]
mod tests {
    use glimpse::mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer};

    use super::{RowSyncOp, row_sync_ops, visible_players};

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

    #[test]
    fn row_sync_ops_reuses_existing_player_ids_with_moves() {
        let current = vec!["one".to_string(), "two".to_string(), "three".to_string()];
        let next = vec!["three".to_string(), "one".to_string()];

        let ops = row_sync_ops(&current, &next);

        assert_eq!(
            ops,
            vec![
                RowSyncOp::Move { from: 2, to: 0 },
                RowSyncOp::Remove { at: 2 },
            ]
        );
    }
}
