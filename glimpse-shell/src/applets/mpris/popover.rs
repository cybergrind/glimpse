#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    factory::FactoryVecDeque,
    gtk::{self, prelude::*},
};

use crate::{
    applets::mpris::components::{
        CurrentPlayer, CurrentPlayerInput, CurrentPlayerOutput, PlayerRowItem, PlayerRowItemInit,
    },
    components::{
        animated_popover::AnimatedPopover, popover_scroll, popover_shell::PopoverShell,
        section_header::SectionHeader,
    },
    services::mpris::{PlaybackStatus, Player, State, model::visible_players},
};

pub struct Popover {
    animation: AnimatedPopover,
    current_player: Controller<CurrentPlayer>,
    rows: FactoryVecDeque<PlayerRowItem>,
    max_rows: usize,
    show_artwork: bool,
    state: State,
    empty_visible: bool,
    other_players_visible: bool,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub max_rows: usize,
    pub show_artwork: bool,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    Update(State),
    Reconfigure { max_rows: usize, show_artwork: bool },
    CurrentPlayerOutput(CurrentPlayerOutput),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Opened,
    Closed,
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

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "mpris-popover",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[local_ref]
                    current_player_widget -> gtk::Box {},

                    #[name = "other_players_section"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        add_css_class: "mpris-other-players",

                        #[name = "other_players_header"]
                        #[template]
                        SectionHeader {},

                        #[name = "scroller"]
                        gtk::ScrolledWindow {
                            set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                            set_vexpand: false,
                            set_propagate_natural_height: true,

                            #[local_ref]
                            rows_widget -> gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 4,
                            },
                        },
                    },

                    #[name = "empty_state"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "empty-state",

                        gtk::Label {
                            add_css_class: "empty-state__title",
                            set_label: "No media playing",
                        },

                        gtk::Label {
                            add_css_class: "empty-state__subtitle",
                            set_label: "Start playback in any MPRIS-compatible player",
                        },
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let current_player = CurrentPlayer::builder()
            .launch(())
            .forward(sender.input_sender(), PopoverInput::CurrentPlayerOutput);
        let current_player_widget = current_player.widget().clone();

        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let rows = FactoryVecDeque::builder()
            .launch(rows_box.clone())
            .forward(sender.output_sender(), |output| output);
        let rows_widget = rows_box.clone();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        widgets
            .other_players_header
            .title
            .set_label("Other players");
        popover_scroll::install_half_monitor_limit(&widgets.root, &widgets.scroller, &init.parent);

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(PopoverOutput::Opened);
        });

        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(PopoverOutput::Closed);
        });

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            current_player,
            rows,
            max_rows: init.max_rows,
            show_artwork: init.show_artwork,
            state: State::default(),
            empty_visible: true,
            other_players_visible: false,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.animation.toggle(),
            PopoverInput::Update(state) => self.sync_state(state),
            PopoverInput::Reconfigure {
                max_rows,
                show_artwork,
            } => {
                self.max_rows = max_rows;
                self.show_artwork = show_artwork;
                self.sync_state(self.state.clone());
            }
            PopoverInput::CurrentPlayerOutput(output) => {
                let _ = sender.output(output.into());
            }
        }
    }

    fn post_view() {
        other_players_section.set_visible(model.other_players_visible);
        empty_state.set_visible(model.empty_visible);
    }
}

impl Popover {
    fn sync_state(&mut self, state: State) {
        let visible = visible_players(&state.snapshot.players);
        let current = state
            .snapshot
            .current_player
            .clone()
            .filter(|player| player.playback_status != PlaybackStatus::Stopped)
            .or_else(|| visible.first().cloned());
        let other_players = other_players(visible, current.as_ref(), self.max_rows);

        self.empty_visible = current.is_none() && other_players.is_empty();
        self.other_players_visible = !other_players.is_empty();
        self.current_player.emit(CurrentPlayerInput::Update {
            player: current.clone(),
            show_artwork: self.show_artwork,
        });
        self.sync_rows(other_players);
        self.state = state;
    }

    fn sync_rows(&mut self, players: Vec<Player>) {
        let next_ids = players
            .iter()
            .map(|player| player.player_id.clone())
            .collect::<Vec<_>>();
        let mut guard = self.rows.guard();
        let current_ids = guard
            .iter()
            .map(|row| row.key().to_string())
            .collect::<Vec<_>>();

        for op in row_sync_ops(&current_ids, &next_ids) {
            match op {
                RowSyncOp::Move { from, to } => guard.move_to(from, to),
                RowSyncOp::Insert { at } => {
                    guard.insert(
                        at,
                        PlayerRowItemInit {
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
            guard[index].sync_player(player, self.show_artwork);
        }
    }
}

impl From<CurrentPlayerOutput> for PopoverOutput {
    fn from(output: CurrentPlayerOutput) -> Self {
        match output {
            CurrentPlayerOutput::Previous { player_id } => Self::Previous { player_id },
            CurrentPlayerOutput::PlayPause { player_id } => Self::PlayPause { player_id },
            CurrentPlayerOutput::Next { player_id } => Self::Next { player_id },
            CurrentPlayerOutput::Raise { player_id } => Self::Raise { player_id },
        }
    }
}

fn other_players(players: Vec<Player>, current: Option<&Player>, max_rows: usize) -> Vec<Player> {
    let current_id = current.map(|player| player.player_id.as_str());
    let limit = max_rows.saturating_sub(usize::from(current.is_some()));

    players
        .into_iter()
        .filter(|player| Some(player.player_id.as_str()) != current_id)
        .take(limit)
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn player(player_id: &str, status: PlaybackStatus) -> Player {
        Player {
            player_id: player_id.into(),
            playback_status: status,
            ..Default::default()
        }
    }

    #[test]
    fn other_players_excludes_current_and_respects_total_limit() {
        let current = player("spotify", PlaybackStatus::Playing);
        let players = vec![
            current.clone(),
            player("firefox", PlaybackStatus::Paused),
            player("mpv", PlaybackStatus::Paused),
        ];

        let ids = other_players(players, Some(&current), 2)
            .into_iter()
            .map(|player| player.player_id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["firefox"]);
    }

    #[test]
    fn row_sync_ops_reuses_existing_player_ids() {
        let ops = row_sync_ops(
            &["spotify".into(), "firefox".into()],
            &["firefox".into(), "mpv".into()],
        );

        assert_eq!(
            ops,
            vec![
                RowSyncOp::Move { from: 1, to: 0 },
                RowSyncOp::Insert { at: 1 },
                RowSyncOp::Remove { at: 2 },
            ]
        );
    }
}
