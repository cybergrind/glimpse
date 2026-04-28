#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::device_list::{DeviceList, DeviceListInit, DeviceListInput, DeviceListItem},
    services::network::NetworkDevice,
};

pub struct WiredSection {
    visible: bool,
    list: Controller<DeviceList<()>>,
    items: Vec<DeviceListItem<()>>,
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
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            #[watch]
            set_visible: model.visible,

            gtk::Separator {
                set_orientation: gtk::Orientation::Horizontal,
            },

            #[local_ref]
            list_widget -> gtk::Box {},
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list = DeviceList::builder()
            .launch(DeviceListInit {
                header: Some("Wired".into()),
                items: Vec::new(),
            })
            .detach();
        let list_widget = list.widget().clone();

        let model = WiredSection {
            visible: false,
            list,
            items: Vec::new(),
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            WiredSectionInput::Update(devices) => {
                let items = build_wired_items(&devices);
                self.visible = !items.is_empty();
                if self.items != items {
                    self.list.emit(DeviceListInput::Update(items.clone()));
                    self.items = items;
                }
            }
        }
    }
}

fn build_wired_items(devices: &[NetworkDevice]) -> Vec<DeviceListItem<()>> {
    visible_wired_devices(devices)
        .into_iter()
        .map(|device| DeviceListItem {
            id: wired_key(device).to_owned(),
            icon: "network-wired-symbolic".into(),
            label: device.interface.clone(),
            status: wired_info(device),
            busy: false,
            tooltip: Some(wired_tooltip(device)),
            active: device.state == "connected",
            visible: true,
            command: None,
        })
        .collect()
}

fn visible_wired_devices(devices: &[NetworkDevice]) -> Vec<&NetworkDevice> {
    devices
        .iter()
        .filter(|device| device.device_type == "ethernet")
        .collect()
}

fn wired_key(device: &NetworkDevice) -> &str {
    if device.path.is_empty() {
        &device.interface
    } else {
        &device.path
    }
}

fn wired_info(device: &NetworkDevice) -> String {
    if device.state == "connected" {
        if device.speed > 0 {
            format!("{} Mbps", device.speed)
        } else {
            "Connected".into()
        }
    } else if device.carrier.unwrap_or(false) {
        "Cable connected".into()
    } else {
        "Disconnected".into()
    }
}

fn wired_tooltip(device: &NetworkDevice) -> String {
    if device
        .driver
        .as_deref()
        .is_some_and(|driver| !driver.is_empty())
    {
        format!(
            "{} - {}",
            device.interface,
            device.driver.as_deref().unwrap()
        )
    } else {
        device.interface.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wired_devices_are_filtered_and_labeled() {
        let ethernet = NetworkDevice {
            path: "/dev/eth0".into(),
            interface: "eth0".into(),
            device_type: "ethernet".into(),
            state: "connected".into(),
            speed: 1000,
            ..NetworkDevice::default()
        };
        let wifi = NetworkDevice {
            path: "/dev/wlan0".into(),
            interface: "wlan0".into(),
            device_type: "wifi".into(),
            ..NetworkDevice::default()
        };

        let items = build_wired_items(&[wifi, ethernet]);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "eth0");
        assert_eq!(items[0].status, "1000 Mbps");
        assert!(items[0].active);
    }
}
