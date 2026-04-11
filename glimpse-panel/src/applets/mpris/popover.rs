use std::collections::HashMap;

use glimpse::mpris::protocol::MprisPlayer;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::player_row::{
    MprisPlayerRow, MprisPlayerRowInit, MprisPlayerRowInput, MprisPlayerRowOutput,
};

pub struct MprisPopover {
    popover: gtk::Popover,
    rows_box: gtk::Box,
    empty_label: gtk::Label,
    max_rows: usize,
    show_artwork: bool,
    rows: HashMap<String, Controller<MprisPlayerRow>>,
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
    RowOutput(MprisPlayerRowOutput),
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

impl MprisPopover {
    fn sync_rows(&mut self, players: Vec<MprisPlayer>, sender: &ComponentSender<Self>) {
        let players = visible_players(players, self.max_rows);
        self.empty_label.set_visible(players.is_empty());

        let next_ids = players
            .iter()
            .map(|player| player.player_id.as_str())
            .collect::<Vec<_>>();
        let to_remove = self
            .rows
            .keys()
            .filter(|player_id| !next_ids.contains(&player_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        for player_id in to_remove {
            if let Some(row) = self.rows.remove(&player_id) {
                self.rows_box.remove(row.widget());
            }
        }

        for player in &players {
            if let Some(row) = self.rows.get_mut(&player.player_id) {
                row.emit(MprisPlayerRowInput::Update(player.clone()));
            } else {
                let row = MprisPlayerRow::builder()
                    .launch(MprisPlayerRowInit {
                        player: player.clone(),
                        show_artwork: self.show_artwork,
                    })
                    .forward(sender.input_sender(), MprisPopoverInput::RowOutput);
                self.rows_box.append(row.widget());
                self.rows.insert(player.player_id.clone(), row);
            }
        }

        let mut previous: Option<gtk::Widget> = None;
        for player in &players {
            let Some(row) = self.rows.get(&player.player_id) else {
                continue;
            };
            self.rows_box
                .reorder_child_after(row.widget(), previous.as_ref());
            previous = Some(row.widget().clone().upcast());
        }
    }
}

impl SimpleComponent for MprisPopover {
    type Init = MprisPopoverInit;
    type Input = MprisPopoverInput;
    type Output = MprisPopoverOutput;
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Popover::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("mpris-popover");

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let empty_label = gtk::Label::new(Some("No media players"));
        empty_label.set_halign(gtk::Align::Center);
        empty_label.set_valign(gtk::Align::Center);
        body.append(&empty_label);

        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        body.append(&rows_box);

        root.set_child(Some(&body));

        let model = MprisPopover {
            popover: root.clone(),
            rows_box,
            empty_label,
            max_rows: init.max_rows,
            show_artwork: init.show_artwork,
            rows: HashMap::new(),
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            MprisPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            MprisPopoverInput::UpdatePlayers(players) => {
                self.sync_rows(players, &sender);
            }
            MprisPopoverInput::RowOutput(output) => {
                let mapped = match output {
                    MprisPlayerRowOutput::Previous { player_id } => {
                        MprisPopoverOutput::Previous { player_id }
                    }
                    MprisPlayerRowOutput::PlayPause { player_id } => {
                        MprisPopoverOutput::PlayPause { player_id }
                    }
                    MprisPlayerRowOutput::Next { player_id } => {
                        MprisPopoverOutput::Next { player_id }
                    }
                    MprisPlayerRowOutput::Raise { player_id } => {
                        MprisPopoverOutput::Raise { player_id }
                    }
                };
                let _ = sender.output(mapped);
            }
        }
    }
}
