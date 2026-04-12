#![allow(unused_assignments)]

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use glimpse::network::protocol::NetworkActiveAction;
use glimpse::network::provider::SavedVpn;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::{NetworkAction, NetworkActionSender};

pub struct VpnSection {
    visible: bool,
    vpn_box: gtk::Box,
    rows: HashMap<String, VpnRow>,
    on_action: NetworkActionSender,
}

#[derive(Debug)]
pub enum VpnSectionInput {
    Update {
        vpns: Vec<SavedVpn>,
        active_action: Option<NetworkActiveAction>,
    },
}

struct VpnRow {
    button: gtk::Button,
    spinner: gtk::Spinner,
    state: Rc<RefCell<SavedVpn>>,
}

#[relm4::component(pub)]
impl SimpleComponent for VpnSection {
    type Init = ();
    type Input = VpnSectionInput;
    type Output = NetworkAction;

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

                gtk::Label {
                    set_label: "VPN",
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                    add_css_class: "net-section-title",
                },
            },

            #[name(vpn_box)]
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
        let sender = sender.clone();
        let on_action: NetworkActionSender = Rc::new(move |action| {
            let _ = sender.output(action);
        });

        let model = VpnSection {
            visible: false,
            vpn_box: gtk::Box::new(gtk::Orientation::Vertical, 0),
            rows: HashMap::new(),
            on_action,
        };
        let widgets = view_output!();

        let mut model = model;
        model.vpn_box = widgets.vpn_box.clone();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let VpnSectionInput::Update {
            vpns,
            active_action,
        } = message;

        self.visible = !vpns.is_empty();

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
                row.update(vpn, active_action.as_ref());
                reorder(&self.vpn_box, &row.button, &self.rows, &vpns, index);
            } else {
                let row = VpnRow::new(vpn, active_action.as_ref(), self.on_action.clone());
                self.vpn_box.append(&row.button);
                reorder(&self.vpn_box, &row.button, &self.rows, &vpns, index);
                self.rows.insert(vpn.uuid.clone(), row);
            }
        }
    }
}

impl VpnRow {
    fn new(
        vpn: &SavedVpn,
        active_action: Option<&NetworkActiveAction>,
        on_action: NetworkActionSender,
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
                on_action(action_for_vpn_click(&state.borrow()));
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

fn action_for_vpn_click(vpn: &SavedVpn) -> NetworkAction {
    if vpn.active {
        NetworkAction::Disconnect {
            uuid: vpn.uuid.clone(),
        }
    } else {
        NetworkAction::ConnectSaved {
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
    use crate::applets::network::components::NetworkAction;

    #[test]
    fn click_action_uses_latest_vpn_state() {
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
            action_for_vpn_click(&active),
            NetworkAction::Disconnect {
                uuid: "vpn-uuid".into(),
            }
        );
        assert_eq!(
            action_for_vpn_click(&inactive),
            NetworkAction::ConnectSaved {
                uuid: "vpn-uuid".into(),
            }
        );
    }
}
