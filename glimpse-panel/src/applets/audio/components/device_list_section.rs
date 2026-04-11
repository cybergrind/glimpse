use glimpse::providers::audio::{AudioDevice, DeviceList};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque},
    gtk::{self, prelude::*},
};

use super::device_row::{DeviceRow, DeviceRowInput, DeviceRowOutput};

pub struct DeviceListSection {
    expanded: bool,
    chevron: gtk::Label,
    rows_box: gtk::Box,
    rows: FactoryVecDeque<AudioDeviceRowItem>,
}

pub struct DeviceListSectionInit {
    pub title: String,
}

#[derive(Debug)]
pub enum DeviceListSectionInput {
    Update(DeviceList),
    ToggleExpanded,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceListSectionOutput {
    Selected(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowSyncOp {
    Move { from: usize, to: usize },
    Insert { at: usize },
    Remove { at: usize },
}

pub struct AudioDeviceRowItem {
    key: String,
    row: Controller<DeviceRow>,
}

impl AudioDeviceRowItem {
    fn key(&self) -> &str {
        &self.key
    }

    fn sync_device(&mut self, device: AudioDevice) {
        self.key = device.name.clone();
        self.row.emit(DeviceRowInput::Update(device));
    }
}

impl FactoryComponent for AudioDeviceRowItem {
    type Init = AudioDevice;
    type Input = DeviceRowInput;
    type Output = DeviceListSectionOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Button;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let key = init.name.clone();
        let row = DeviceRow::builder()
            .launch(init)
            .forward(sender.output_sender(), map_row_output);

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

    fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        let DeviceRowInput::Update(device) = msg else {
            return;
        };
        self.sync_device(device);
    }
}

#[relm4::component(pub)]
impl SimpleComponent for DeviceListSection {
    type Init = DeviceListSectionInit;
    type Input = DeviceListSectionInput;
    type Output = DeviceListSectionOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,

            gtk::Button {
                add_css_class: "flat",
                add_css_class: "device-btn",
                connect_clicked => DeviceListSectionInput::ToggleExpanded,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    add_css_class: "device-header",

                    #[name(title_label)]
                    gtk::Label {
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
                add_css_class: "device-list",
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
        widgets.title_label.set_label(&init.title);
        let rows = FactoryVecDeque::builder()
            .launch(widgets.rows_box.clone())
            .forward(sender.output_sender(), |output| output);

        let model = DeviceListSection {
            expanded: false,
            chevron: widgets.chevron.clone(),
            rows_box: widgets.rows_box.clone(),
            rows,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            DeviceListSectionInput::Update(devices) => self.sync_rows(&devices),
            DeviceListSectionInput::ToggleExpanded => {
                self.expanded = !self.expanded;
                self.rows_box.set_visible(self.expanded);
                self.chevron
                    .set_label(if self.expanded { "⌄" } else { "›" });
            }
        }
    }
}

impl DeviceListSection {
    fn sync_rows(&mut self, devices: &DeviceList) {
        let devices = devices.iter().cloned().collect::<Vec<_>>();
        let next_keys = devices
            .iter()
            .map(|device| device.name.clone())
            .collect::<Vec<_>>();
        let mut guard = self.rows.guard();
        let current_keys = guard
            .iter()
            .map(|row| row.key().to_string())
            .collect::<Vec<_>>();

        for op in row_sync_ops(&current_keys, &next_keys) {
            match op {
                RowSyncOp::Move { from, to } => guard.move_to(from, to),
                RowSyncOp::Insert { at } => {
                    guard.insert(at, devices[at].clone());
                }
                RowSyncOp::Remove { at } => {
                    guard.remove(at);
                }
            }
        }

        for (index, device) in devices.into_iter().enumerate() {
            guard[index].sync_device(device);
        }
    }
}

fn map_row_output(output: DeviceRowOutput) -> DeviceListSectionOutput {
    match output {
        DeviceRowOutput::Selected(name) => DeviceListSectionOutput::Selected(name),
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
