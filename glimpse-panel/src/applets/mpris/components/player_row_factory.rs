use glimpse::mpris::protocol::MprisPlayer;
use relm4::{
    Component, ComponentController, Controller,
    factory::{DynamicIndex, FactoryComponent, FactorySender},
    gtk,
};

use super::player_row::{
    MprisPlayerRow, MprisPlayerRowInit, MprisPlayerRowInput, MprisPlayerRowOutput,
};
use crate::applets::mpris::popover::MprisPopoverOutput;

#[derive(Debug, Clone)]
pub struct MprisPlayerRowItemInit {
    pub player: MprisPlayer,
    pub show_artwork: bool,
}

pub struct MprisPlayerRowItem {
    player_id: String,
    row: Controller<MprisPlayerRow>,
}

impl MprisPlayerRowItem {
    pub fn key(&self) -> &str {
        &self.player_id
    }
}

impl FactoryComponent for MprisPlayerRowItem {
    type Init = MprisPlayerRowItemInit;
    type Input = MprisPlayerRowInput;
    type Output = MprisPopoverOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let player_id = init.player.player_id.clone();
        let row = MprisPlayerRow::builder()
            .launch(MprisPlayerRowInit {
                player: init.player,
                show_artwork: init.show_artwork,
            })
            .forward(sender.output_sender(), row_output_to_popover_output);

        Self { player_id, row }
    }

    fn init_root(&self) -> Self::Root {
        self.row.widget().clone()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        _root: Self::Root,
        _returned_widget: &gtk::Widget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
    }

    fn update(&mut self, message: Self::Input, _sender: FactorySender<Self>) {
        let MprisPlayerRowInput::Update(player) = message;
        self.player_id = player.player_id.clone();
        debug_assert_eq!(self.key(), player.player_id.as_str());
        self.row.emit(MprisPlayerRowInput::Update(player));
    }
}

fn row_output_to_popover_output(output: MprisPlayerRowOutput) -> MprisPopoverOutput {
    match output {
        MprisPlayerRowOutput::Previous { player_id } => MprisPopoverOutput::Previous { player_id },
        MprisPlayerRowOutput::PlayPause { player_id } => {
            MprisPopoverOutput::PlayPause { player_id }
        }
        MprisPlayerRowOutput::Next { player_id } => MprisPopoverOutput::Next { player_id },
        MprisPlayerRowOutput::Raise { player_id } => MprisPopoverOutput::Raise { player_id },
    }
}
