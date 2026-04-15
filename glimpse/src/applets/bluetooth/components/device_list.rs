#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque},
    gtk::{self, prelude::*},
};

use super::{
    BluetoothDeviceAction, BluetoothDeviceRow, BluetoothDeviceRowInput, BluetoothDeviceRowOutput,
    BtDevice,
};

pub struct BluetoothDeviceList {
    connected_count: u32,
    empty_label: gtk::Label,
    rows: FactoryVecDeque<BluetoothDeviceRowItem>,
}

#[derive(Debug)]
pub enum BluetoothDeviceListInput {
    UpdateDevices(Vec<BtDevice>),
    FinishDeviceAction { address: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothDeviceListOutput {
    ConnectedCount(u32),
    DeviceAction {
        address: String,
        name: String,
        action: BluetoothDeviceAction,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowSyncOp {
    Move { from: usize, to: usize },
    Insert { at: usize },
    Remove { at: usize },
}

pub struct BluetoothDeviceRowItem {
    key: String,
    row: Controller<BluetoothDeviceRow>,
}

impl BluetoothDeviceRowItem {
    fn key(&self) -> &str {
        &self.key
    }

    fn sync_device(&mut self, device: BtDevice) {
        self.key = device.address.clone();
        self.row.emit(BluetoothDeviceRowInput::Update(device));
    }
}

impl FactoryComponent for BluetoothDeviceRowItem {
    type Init = BtDevice;
    type Input = BluetoothDeviceRowInput;
    type Output = BluetoothDeviceListOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Button;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let key = init.address.clone();
        let row = BluetoothDeviceRow::builder()
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
        if let BluetoothDeviceRowInput::Update(device) = msg {
            self.sync_device(device);
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for BluetoothDeviceList {
    type Init = ();
    type Input = BluetoothDeviceListInput;
    type Output = BluetoothDeviceListOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            #[name(empty_label)]
            gtk::Label {
                set_visible: true,
                set_label: "No devices",
                set_halign: gtk::Align::Start,
                add_css_class: "bt-empty",
            },

            gtk::ScrolledWindow {
                set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                set_max_content_height: 300,
                set_propagate_natural_height: true,

                #[local_ref]
                rows_box -> gtk::Box {},
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        rows_box.add_css_class("bt-device-list");

        let rows = FactoryVecDeque::builder()
            .launch(rows_box.clone())
            .forward(sender.output_sender(), |output| output);

        let widgets = view_output!();

        let model = BluetoothDeviceList {
            connected_count: 0,
            empty_label: widgets.empty_label.clone(),
            rows,
        };

        model.empty_label.set_visible(true);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BluetoothDeviceListInput::UpdateDevices(devices) => {
                self.sync_rows(devices);
                let connected_count = self.connected_count;
                let _ = sender.output(BluetoothDeviceListOutput::ConnectedCount(connected_count));
            }
            BluetoothDeviceListInput::FinishDeviceAction { address } => {
                let guard = self.rows.guard();
                if let Some(row) = guard.iter().find(|row| row.key() == address) {
                    row.row.emit(BluetoothDeviceRowInput::FinishAction);
                }
            }
        }
    }
}

impl BluetoothDeviceList {
    fn sync_rows(&mut self, devices: Vec<BtDevice>) {
        let visible = visible_devices(&devices);
        self.connected_count = visible.iter().filter(|device| device.connected).count() as u32;
        self.empty_label.set_visible(visible.is_empty());

        let next_keys = visible
            .iter()
            .map(|device| device.address.clone())
            .collect::<Vec<_>>();

        let current_keys = {
            let guard = self.rows.guard();
            guard
                .iter()
                .map(BluetoothDeviceRowItem::key)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        };

        let mut guard = self.rows.guard();
        for op in row_sync_ops(&current_keys, &next_keys) {
            match op {
                RowSyncOp::Move { from, to } => guard.move_to(from, to),
                RowSyncOp::Insert { at } => {
                    guard.insert(at, visible[at].clone());
                }
                RowSyncOp::Remove { at } => {
                    guard.remove(at);
                }
            }
        }

        for (index, device) in visible.into_iter().enumerate() {
            guard[index].sync_device(device);
        }
    }
}

fn map_row_output(output: BluetoothDeviceRowOutput) -> BluetoothDeviceListOutput {
    match output {
        BluetoothDeviceRowOutput::Action {
            address,
            name,
            action,
        } => BluetoothDeviceListOutput::DeviceAction {
            address,
            name,
            action,
        },
    }
}

fn visible_devices(devices: &[BtDevice]) -> Vec<BtDevice> {
    let mut visible = devices
        .iter()
        .filter(|device| is_visible_device(device))
        .cloned()
        .collect::<Vec<_>>();
    visible.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.paired.cmp(&a.paired))
            .then(b.rssi.unwrap_or(i16::MIN).cmp(&a.rssi.unwrap_or(i16::MIN)))
    });
    visible
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

fn is_visible_device(dev: &BtDevice) -> bool {
    if dev.name.is_empty() || looks_like_mac(&dev.name) {
        return dev.connected || dev.paired || dev.trusted;
    }
    dev.connected || dev.paired || dev.trusted || dev.rssi.is_some()
}

fn looks_like_mac(s: &str) -> bool {
    let s = s.trim();
    if s.len() != 17 {
        return false;
    }
    let sep = if s.contains(':') {
        ':'
    } else if s.contains('-') {
        '-'
    } else {
        return false;
    };
    let parts: Vec<&str> = s.split(sep).collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(
        address: &str,
        connected: bool,
        paired: bool,
        trusted: bool,
        rssi: Option<i16>,
        name: &str,
    ) -> BtDevice {
        BtDevice {
            address: address.into(),
            name: name.into(),
            icon: "bluetooth-symbolic".into(),
            device_type: "Device".into(),
            paired,
            trusted,
            connected,
            battery: None,
            rssi,
        }
    }

    #[test]
    fn filters_and_orders_devices_like_previous_implementation() {
        let devices = vec![
            device("1", false, false, false, Some(-80), "Mouse"),
            device("2", true, false, false, None, "Speaker"),
            device("3", false, true, false, Some(-20), "Keyboard"),
            device("4", false, false, false, None, "Unpaired"),
        ];

        let visible = visible_devices(&devices);

        assert_eq!(
            visible
                .iter()
                .map(|d| d.address.as_str())
                .collect::<Vec<_>>(),
            vec!["2", "3", "1"]
        );
    }

    #[test]
    fn row_sync_ops_preserves_move_insert_and_remove_order() {
        let ops = row_sync_ops(
            &["a".into(), "b".into(), "c".into()],
            &["b".into(), "d".into(), "a".into()],
        );
        assert_eq!(
            ops,
            vec![
                RowSyncOp::Move { from: 1, to: 0 },
                RowSyncOp::Insert { at: 1 },
                RowSyncOp::Remove { at: 3 },
            ]
        );
    }
}
