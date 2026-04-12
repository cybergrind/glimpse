#![allow(unused_assignments)]

use glimpse::brightness::provider::BrightnessDisplay;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque},
    gtk::{self, prelude::*},
};

use super::display_row::{
    BrightnessDisplayRow, BrightnessDisplayRowInput, BrightnessDisplayRowOutput,
};

pub struct BrightnessDisplayList {
    empty_label: gtk::Label,
    rows: FactoryVecDeque<BrightnessRowItem>,
}

#[derive(Debug)]
pub enum BrightnessDisplayListInput {
    Update(Vec<BrightnessDisplay>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrightnessDisplayListOutput {
    SetDisplayPercent { display_id: String, percent: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowSyncOp {
    Move { from: usize, to: usize },
    Insert { at: usize },
    Remove { at: usize },
}

pub struct BrightnessRowItem {
    key: String,
    row: Controller<BrightnessDisplayRow>,
}

impl BrightnessRowItem {
    fn key(&self) -> &str {
        &self.key
    }

    fn sync_display(&mut self, display: BrightnessDisplay) {
        self.key = display.id.clone();
        self.row.emit(BrightnessDisplayRowInput::Update(display));
    }
}

impl FactoryComponent for BrightnessRowItem {
    type Init = BrightnessDisplay;
    type Input = BrightnessDisplayRowInput;
    type Output = BrightnessDisplayListOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let key = init.id.clone();
        let row = BrightnessDisplayRow::builder().launch(init).forward(
            sender.output_sender(),
            |output| match output {
                BrightnessDisplayRowOutput::SetPercent {
                    display_id,
                    percent,
                } => BrightnessDisplayListOutput::SetDisplayPercent {
                    display_id,
                    percent,
                },
            },
        );

        Self { key, row }
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
        let BrightnessDisplayRowInput::Update(display) = message else {
            return;
        };
        self.sync_display(display);
    }
}

#[relm4::component(pub)]
impl SimpleComponent for BrightnessDisplayList {
    type Init = ();
    type Input = BrightnessDisplayListInput;
    type Output = BrightnessDisplayListOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,

            #[name(empty_label)]
            gtk::Label {
                set_label: "No controllable displays",
                set_halign: gtk::Align::Start,
            },

            #[name(rows_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        let rows = FactoryVecDeque::builder()
            .launch(widgets.rows_box.clone())
            .forward(sender.output_sender(), |output| output);
        let model = BrightnessDisplayList {
            empty_label: widgets.empty_label.clone(),
            rows,
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            BrightnessDisplayListInput::Update(displays) => {
                self.empty_label.set_visible(displays.is_empty());
                sync_rows(&mut self.rows, displays);
            }
        }
    }
}

fn sync_rows(rows: &mut FactoryVecDeque<BrightnessRowItem>, displays: Vec<BrightnessDisplay>) {
    let next_keys = displays
        .iter()
        .map(|display| display.id.clone())
        .collect::<Vec<_>>();
    let mut guard = rows.guard();
    let current_keys = guard
        .iter()
        .map(|row| row.key().to_string())
        .collect::<Vec<_>>();

    for op in row_sync_ops(&current_keys, &next_keys) {
        match op {
            RowSyncOp::Move { from, to } => guard.move_to(from, to),
            RowSyncOp::Insert { at } => {
                guard.insert(at, displays[at].clone());
            }
            RowSyncOp::Remove { at } => {
                guard.remove(at);
            }
        }
    }

    for (index, display) in displays.into_iter().enumerate() {
        guard[index].sync_display(display);
    }
}

fn row_sync_ops(current_keys: &[String], next_keys: &[String]) -> Vec<RowSyncOp> {
    let mut working = current_keys.to_vec();
    let mut ops = Vec::new();

    for (target_index, key) in next_keys.iter().enumerate() {
        if working.get(target_index) == Some(key) {
            continue;
        }

        if let Some(found_index) = working.iter().position(|current| current == key) {
            let moved = working.remove(found_index);
            working.insert(target_index, moved);
            ops.push(RowSyncOp::Move {
                from: found_index,
                to: target_index,
            });
        } else {
            working.insert(target_index, key.clone());
            ops.push(RowSyncOp::Insert { at: target_index });
        }
    }

    while working.len() > next_keys.len() {
        working.remove(next_keys.len());
        ops.push(RowSyncOp::Remove {
            at: next_keys.len(),
        });
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::{RowSyncOp, row_sync_ops};

    #[test]
    fn row_sync_ops_moves_and_removes_by_display_id() {
        let current = vec!["a".into(), "b".into(), "c".into()];
        let next = vec!["b".into(), "d".into(), "c".into()];

        assert_eq!(
            row_sync_ops(&current, &next),
            vec![
                RowSyncOp::Move { from: 1, to: 0 },
                RowSyncOp::Insert { at: 1 },
                RowSyncOp::Move { from: 3, to: 2 },
                RowSyncOp::Remove { at: 3 },
            ]
        );
    }
}
