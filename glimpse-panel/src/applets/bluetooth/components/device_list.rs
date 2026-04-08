use std::collections::{HashMap, HashSet};

use relm4::gtk::{self, prelude::*};

use super::{BluetoothCommandSender, BtDevice, device_row::DeviceRow};

pub struct DeviceList {
    device_box: gtk::Box,
    empty_label: gtk::Label,
    rows: HashMap<String, DeviceRow>,
    on_command: BluetoothCommandSender,
}

impl DeviceList {
    pub fn new(on_command: BluetoothCommandSender) -> (Self, gtk::Box) {
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let empty_label = gtk::Label::new(Some("No devices"));
        empty_label.set_halign(gtk::Align::Start);
        empty_label.add_css_class("bt-empty");
        outer.append(&empty_label);

        let device_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        device_box.add_css_class("bt-device-list");

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_max_content_height(300);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&device_box));
        outer.append(&scroll);

        let model = Self {
            device_box,
            empty_label,
            rows: HashMap::new(),
            on_command,
        };

        (model, outer)
    }

    pub fn update(&mut self, devices: Vec<BtDevice>) -> u32 {
        let mut visible: Vec<&BtDevice> = devices.iter().filter(|d| is_visible_device(d)).collect();
        visible.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then(b.paired.cmp(&a.paired))
                .then(b.rssi.unwrap_or(i16::MIN).cmp(&a.rssi.unwrap_or(i16::MIN)))
        });

        let connected_count = visible.iter().filter(|d| d.connected).count() as u32;

        let visible_addrs: HashSet<&str> = visible.iter().map(|d| d.address.as_str()).collect();
        let to_remove: Vec<String> = self
            .rows
            .keys()
            .filter(|addr| !visible_addrs.contains(addr.as_str()))
            .cloned()
            .collect();
        for addr in to_remove {
            if let Some(row) = self.rows.remove(&addr) {
                row.popover_menu.unparent();
                self.device_box.remove(&row.button);
            }
        }

        for (i, dev) in visible.iter().enumerate() {
            if let Some(row) = self.rows.get(&dev.address) {
                row.update(dev);
                if i == 0 {
                    self.device_box
                        .reorder_child_after(&row.button, Option::<&gtk::Widget>::None);
                } else if let Some(prev) = visible.get(i - 1) {
                    if let Some(prev_row) = self.rows.get(&prev.address) {
                        self.device_box
                            .reorder_child_after(&row.button, Some(&prev_row.button));
                    }
                }
            } else {
                let row = DeviceRow::new(dev, self.on_command.clone());
                self.device_box.append(&row.button);
                self.rows.insert(dev.address.clone(), row);
            }
        }

        self.empty_label.set_visible(visible.is_empty());
        connected_count
    }

    pub fn finish_action(&self, address: &str) {
        if let Some(row) = self.rows.get(address) {
            row.finish_action();
        }
    }
}

fn is_visible_device(dev: &BtDevice) -> bool {
    if dev.name.is_empty() || looks_like_mac(&dev.name) {
        return dev.connected || dev.paired || dev.trusted;
    }
    dev.connected || dev.paired || dev.trusted || dev.rssi.is_some()
}

fn looks_like_mac(s: &str) -> bool {
    let s = s.trim();
    if s.len() < 11 {
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
