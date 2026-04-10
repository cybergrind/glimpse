use glimpse::providers::network::NetworkDevice;
use relm4::gtk::{self, prelude::*};

pub struct WiredSection {
    section: gtk::Box,
    device_box: gtk::Box,
}

impl WiredSection {
    pub fn new() -> (Self, gtk::Box) {
        let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        section.set_visible(false);

        section.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("net-section-header");
        let title = gtk::Label::new(Some("Wired"));
        title.set_halign(gtk::Align::Start);
        title.set_hexpand(true);
        title.add_css_class("net-section-title");
        header.append(&title);
        section.append(&header);

        let device_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        section.append(&device_box);

        (
            Self {
                section: section.clone(),
                device_box,
            },
            section,
        )
    }

    pub fn update(&mut self, devices: &[NetworkDevice]) {
        let wired_devices: Vec<&NetworkDevice> = devices
            .iter()
            .filter(|device| device.device_type == "ethernet")
            .collect();
        self.section.set_visible(!wired_devices.is_empty());

        let mut child = self.device_box.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            self.device_box.remove(&widget);
        }

        for device in wired_devices {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

            let icon = gtk::Image::from_icon_name("network-wired-symbolic");
            icon.set_pixel_size(16);
            icon.set_valign(gtk::Align::Center);
            icon.add_css_class("net-ap-icon");
            row.append(&icon);

            let name = gtk::Label::new(Some(&device.interface));
            name.set_hexpand(true);
            name.set_halign(gtk::Align::Start);
            row.append(&name);

            let info = if device.state == "connected" {
                if device.speed > 0 {
                    format!("{} Mbps", device.speed)
                } else {
                    "Connected".into()
                }
            } else if device.carrier.unwrap_or(false) {
                "Cable connected".into()
            } else {
                "Disconnected".into()
            };
            let info_label = gtk::Label::new(Some(&info));
            info_label.add_css_class("net-dim");
            row.append(&info_label);

            let button = gtk::Button::new();
            button.set_child(Some(&row));
            button.add_css_class("flat");
            button.add_css_class("net-device-btn");
            button.set_sensitive(false);
            self.device_box.append(&button);
        }
    }
}
