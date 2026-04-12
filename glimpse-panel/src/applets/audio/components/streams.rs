use glimpse::providers::audio::AudioStream;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque},
    gtk::{self, prelude::*},
};

use super::stream_item::{StreamItem, StreamItemInit, StreamItemInput, StreamItemOutput};

pub struct StreamList {
    expanded: bool,
    header_label: gtk::Label,
    chevron: gtk::Label,
    rows_box: gtk::Box,
    max_volume: f64,
    rows: FactoryVecDeque<AudioStreamRowItem>,
}

pub struct StreamListInit {
    pub max_volume: f64,
    pub show_streams: bool,
}

#[derive(Debug)]
pub enum StreamListInput {
    Update(Vec<AudioStream>),
    ToggleExpanded,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamListOutput {
    ToggleMute(u64),
    SetVolume { stream_id: u64, volume: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowSyncOp {
    Move { from: usize, to: usize },
    Insert { at: usize },
    Remove { at: usize },
}

pub struct AudioStreamRowItem {
    stream_id: u64,
    row: Controller<StreamItem>,
}

impl AudioStreamRowItem {
    fn key(&self) -> u64 {
        self.stream_id
    }

    fn sync_stream(&mut self, stream: AudioStream) {
        debug_assert_eq!(self.stream_id, stream.index);
        self.stream_id = stream.index;
        self.row.emit(StreamItemInput::Update(stream));
    }
}

impl FactoryComponent for AudioStreamRowItem {
    type Init = StreamItemInit;
    type Input = StreamItemInput;
    type Output = StreamListOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let stream_id = init.stream.index;
        let row = StreamItem::builder()
            .launch(init)
            .forward(sender.output_sender(), map_stream_item_output);

        Self { stream_id, row }
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

    fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        let StreamItemInput::Update(stream) = msg;
        self.sync_stream(stream);
    }
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for StreamList {
    type Init = StreamListInit;
    type Input = StreamListInput;
    type Output = StreamListOutput;

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,

            gtk::Button {
                add_css_class: "flat",
                add_css_class: "device-btn",
                connect_clicked => StreamListInput::ToggleExpanded,

                gtk::Box {
                    set_spacing: 8,
                    add_css_class: "device-header",

                    #[name(header_label)]
                    gtk::Label {
                        set_label: "Apps (0)",
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                    },

                    #[name(chevron)]
                    gtk::Label {
                        set_label: "›",
                        add_css_class: "chevron",
                    },
                },
            },

            #[name(rows_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                add_css_class: "streams-box",
                set_visible: false,
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        let rows = FactoryVecDeque::builder()
            .launch(widgets.rows_box.clone())
            .forward(sender.output_sender(), |output| output);
        widgets.root.set_visible(init.show_streams);

        let model = StreamList {
            expanded: false,
            header_label: widgets.header_label.clone(),
            chevron: widgets.chevron.clone(),
            rows_box: widgets.rows_box.clone(),
            max_volume: init.max_volume,
            rows,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            StreamListInput::Update(streams) => self.sync_rows(streams),
            StreamListInput::ToggleExpanded => {
                self.expanded = !self.expanded;
                self.rows_box.set_visible(self.expanded);
                self.chevron
                    .set_label(if self.expanded { "⌄" } else { "›" });
            }
        }
    }
}

impl StreamList {
    fn sync_rows(&mut self, streams: Vec<AudioStream>) {
        self.header_label
            .set_label(&format!("Apps ({})", streams.len()));

        let next_ids = streams
            .iter()
            .map(|stream| stream.index)
            .collect::<Vec<_>>();
        let mut guard = self.rows.guard();
        let current_ids = guard
            .iter()
            .map(AudioStreamRowItem::key)
            .collect::<Vec<_>>();

        for op in row_sync_ops(&current_ids, &next_ids) {
            match op {
                RowSyncOp::Move { from, to } => guard.move_to(from, to),
                RowSyncOp::Insert { at } => {
                    guard.insert(
                        at,
                        StreamItemInit {
                            stream: streams[at].clone(),
                            max_volume: self.max_volume,
                        },
                    );
                }
                RowSyncOp::Remove { at } => {
                    guard.remove(at);
                }
            }
        }

        for (index, stream) in streams.into_iter().enumerate() {
            guard[index].sync_stream(stream);
        }
    }
}

fn map_stream_item_output(output: StreamItemOutput) -> StreamListOutput {
    match output {
        StreamItemOutput::ToggleMute(stream_id) => StreamListOutput::ToggleMute(stream_id),
        StreamItemOutput::SetVolume { stream_id, volume } => {
            StreamListOutput::SetVolume { stream_id, volume }
        }
    }
}

fn row_sync_ops(current_ids: &[u64], next_ids: &[u64]) -> Vec<RowSyncOp> {
    let mut working = current_ids.to_vec();
    let mut ops = Vec::new();

    for (target_index, stream_id) in next_ids.iter().enumerate() {
        if working.get(target_index) == Some(stream_id) {
            continue;
        }

        if let Some(found_index) = working.iter().position(|id| id == stream_id) {
            let moved = working.remove(found_index);
            working.insert(target_index, moved);
            ops.push(RowSyncOp::Move {
                from: found_index,
                to: target_index,
            });
        } else {
            working.insert(target_index, *stream_id);
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
    use super::{RowSyncOp, row_sync_ops};

    #[test]
    fn row_sync_ops_reorders_and_removes_stream_rows() {
        let current = vec![1, 2, 3];
        let next = vec![3, 1];

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
