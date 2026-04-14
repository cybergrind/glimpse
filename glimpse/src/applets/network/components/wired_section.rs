#![allow(unused_assignments)]

use std::collections::{HashMap, HashSet};

use glimpse::network::provider::NetworkDevice;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::wired_row::{WiredRow, WiredRowInput};

pub struct WiredSection {
    visible: bool,
    device_box: gtk::Box,
    rows: HashMap<String, Controller<WiredRow>>,
}

#[derive(Debug)]
pub enum WiredSectionInput {
    Update(Vec<NetworkDevice>),
}

#[relm4::component(pub)]
impl SimpleComponent for WiredSection {
    type Init = ();
    type Input = WiredSectionInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            #[watch]
            set_visible: model.visible,

            gtk::Separator {
                set_orientation: gtk::Orientation::Horizontal,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "net-section-header",
                add_css_class: "section-block__header",

                gtk::Label {
                    set_label: "Wired",
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                    add_css_class: "net-section-title",
                    add_css_class: "section-block__title",
                },
            },

            #[name(device_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = WiredSection {
            visible: false,
            device_box: gtk::Box::new(gtk::Orientation::Vertical, 0),
            rows: HashMap::new(),
        };
        let widgets = view_output!();

        let mut model = model;
        model.device_box = widgets.device_box.clone();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let WiredSectionInput::Update(devices) = message;

        let wired_devices: Vec<&NetworkDevice> = devices
            .iter()
            .filter(|device| device.device_type == "ethernet")
            .collect();
        self.visible = !wired_devices.is_empty();

        let visible_ids: HashSet<&str> = wired_devices
            .iter()
            .map(|device| device.interface.as_str())
            .collect();
        let to_remove = self
            .rows
            .keys()
            .filter(|interface| !visible_ids.contains(interface.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for interface in to_remove {
            if let Some(row) = self.rows.remove(&interface) {
                self.device_box.remove(row.widget());
            }
        }

        for (index, device) in wired_devices.iter().enumerate() {
            if let Some(row) = self.rows.get(&device.interface) {
                row.emit(WiredRowInput::Update((*device).clone()));
            } else {
                let row = WiredRow::builder().launch((*device).clone()).detach();
                self.device_box.append(row.widget());
                self.rows.insert(device.interface.clone(), row);
            }

            reorder(&self.device_box, &self.rows, &wired_devices, index);
        }
    }
}

fn reorder(
    parent: &gtk::Box,
    rows: &HashMap<String, Controller<WiredRow>>,
    devices: &[&NetworkDevice],
    index: usize,
) {
    let current = &devices[index].interface;
    let Some(row) = rows.get(current) else {
        return;
    };

    if index == 0 {
        parent.reorder_child_after(row.widget(), Option::<&gtk::Widget>::None);
    } else if let Some(previous) = devices.get(index - 1) {
        if let Some(previous_row) = rows.get(&previous.interface) {
            parent.reorder_child_after(row.widget(), Some(previous_row.widget()));
        }
    }
}
