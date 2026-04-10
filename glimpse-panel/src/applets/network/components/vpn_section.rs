use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use glimpse::network::protocol::NetworkActiveAction;
use glimpse::providers::network::SavedVpn;
use relm4::gtk::{self, prelude::*};

use super::{NetworkCommand, NetworkCommandSender};

pub struct VpnSection {
    section: gtk::Box,
    vpn_box: gtk::Box,
    rows: HashMap<String, VpnRow>,
    on_command: NetworkCommandSender,
}

struct VpnRow {
    button: gtk::Button,
    spinner: gtk::Spinner,
    state: Rc<RefCell<SavedVpn>>,
}

impl VpnSection {
    pub fn new(on_command: NetworkCommandSender) -> (Self, gtk::Box) {
        let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        section.set_visible(false);

        section.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("net-section-header");
        let title = gtk::Label::new(Some("VPN"));
        title.set_halign(gtk::Align::Start);
        title.set_hexpand(true);
        title.add_css_class("net-section-title");
        header.append(&title);
        section.append(&header);

        let vpn_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        section.append(&vpn_box);

        (
            Self {
                section: section.clone(),
                vpn_box,
                rows: HashMap::new(),
                on_command,
            },
            section,
        )
    }

    pub fn update(&mut self, vpns: &[SavedVpn], active_action: Option<&NetworkActiveAction>) {
        self.section.set_visible(!vpns.is_empty());

        let visible_ids: HashSet<&str> = vpns.iter().map(|vpn| vpn.uuid.as_str()).collect();
        let to_remove: Vec<String> = self
            .rows
            .keys()
            .filter(|uuid| !visible_ids.contains(uuid.as_str()))
            .cloned()
            .collect();
        for uuid in to_remove {
            if let Some(row) = self.rows.remove(&uuid) {
                self.vpn_box.remove(&row.button);
            }
        }

        for (index, vpn) in vpns.iter().enumerate() {
            if let Some(row) = self.rows.get(&vpn.uuid) {
                row.update(vpn, active_action);
                reorder(&self.vpn_box, &row.button, &self.rows, vpns, index);
            } else {
                let row = VpnRow::new(vpn, active_action, self.on_command.clone());
                self.vpn_box.append(&row.button);
                reorder(&self.vpn_box, &row.button, &self.rows, vpns, index);
                self.rows.insert(vpn.uuid.clone(), row);
            }
        }
    }
}

impl VpnRow {
    fn new(
        vpn: &SavedVpn,
        active_action: Option<&NetworkActiveAction>,
        on_command: NetworkCommandSender,
    ) -> Self {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let icon = gtk::Image::from_icon_name("network-vpn-symbolic");
        icon.set_pixel_size(16);
        icon.set_valign(gtk::Align::Center);
        icon.add_css_class("net-ap-icon");
        row.append(&icon);

        let name = gtk::Label::new(Some(&vpn.id));
        name.set_hexpand(true);
        name.set_halign(gtk::Align::Start);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_max_width_chars(25);
        row.append(&name);

        let spinner = gtk::Spinner::new();
        spinner.set_size_request(16, 16);
        row.append(&spinner);

        let button = gtk::Button::new();
        button.set_child(Some(&row));
        button.add_css_class("flat");
        button.add_css_class("net-vpn-btn");

        let state = Rc::new(RefCell::new(vpn.clone()));
        {
            let state = state.clone();
            button.connect_clicked(move |_| {
                on_command(command_for_vpn_click(&state.borrow()));
            });
        }

        let row = Self {
            button,
            spinner,
            state,
        };
        row.update(vpn, active_action);
        row
    }

    fn update(&self, vpn: &SavedVpn, active_action: Option<&NetworkActiveAction>) {
        *self.state.borrow_mut() = vpn.clone();
        let action_active = matches!(
            active_action,
            Some(NetworkActiveAction::ConnectSaved { uuid }) if uuid == &vpn.uuid
        ) || matches!(
            active_action,
            Some(NetworkActiveAction::Disconnect { uuid }) if uuid == &vpn.uuid
        );

        self.spinner.set_visible(action_active);
        if action_active {
            self.spinner.start();
        } else {
            self.spinner.stop();
        }

        self.button.set_tooltip_text(Some(if vpn.active {
            "Disconnect VPN"
        } else {
            "Connect VPN"
        }));
    }
}

fn command_for_vpn_click(vpn: &SavedVpn) -> NetworkCommand {
    if vpn.active {
        NetworkCommand::Disconnect {
            uuid: vpn.uuid.clone(),
        }
    } else {
        NetworkCommand::ConnectSaved {
            uuid: vpn.uuid.clone(),
        }
    }
}

fn reorder(
    parent: &gtk::Box,
    child: &gtk::Button,
    rows: &HashMap<String, VpnRow>,
    vpns: &[SavedVpn],
    index: usize,
) {
    if index == 0 {
        parent.reorder_child_after(child, Option::<&gtk::Widget>::None);
    } else if let Some(previous) = vpns.get(index - 1) {
        if let Some(previous_row) = rows.get(&previous.uuid) {
            parent.reorder_child_after(child, Some(&previous_row.button));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::applets::network::components::NetworkCommand;

    #[test]
    fn click_command_uses_latest_vpn_state() {
        let active = SavedVpn {
            id: "Work".into(),
            uuid: "vpn-uuid".into(),
            active: true,
            ..SavedVpn::default()
        };
        let inactive = SavedVpn {
            id: "Work".into(),
            uuid: "vpn-uuid".into(),
            active: false,
            ..SavedVpn::default()
        };

        assert_eq!(
            command_for_vpn_click(&active),
            NetworkCommand::Disconnect {
                uuid: "vpn-uuid".into(),
            }
        );
        assert_eq!(
            command_for_vpn_click(&inactive),
            NetworkCommand::ConnectSaved {
                uuid: "vpn-uuid".into(),
            }
        );
    }
}
