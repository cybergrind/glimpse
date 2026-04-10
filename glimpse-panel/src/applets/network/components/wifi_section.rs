use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use glimpse::network::protocol::NetworkActiveAction;
use glimpse::providers::network::WifiAccessPoint;
use relm4::gtk::{self, prelude::*};

use super::{NetworkCommand, NetworkCommandSender};

pub struct WifiSection {
    empty_label: gtk::Label,
    access_point_box: gtk::Box,
    rows: HashMap<String, WifiRow>,
    on_command: NetworkCommandSender,
}

struct WifiRow {
    button: gtk::Button,
    icon: gtk::Image,
    name_label: gtk::Label,
    lock_icon: gtk::Image,
    spinner: gtk::Spinner,
    popover_menu: gtk::PopoverMenu,
    state: Rc<RefCell<WifiAccessPoint>>,
}

impl WifiSection {
    pub fn new(on_command: NetworkCommandSender) -> (Self, gtk::Box) {
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let empty_label = gtk::Label::new(Some("No access points"));
        empty_label.set_halign(gtk::Align::Start);
        empty_label.add_css_class("net-empty");
        outer.append(&empty_label);

        let access_point_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_max_content_height(300);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&access_point_box));
        outer.append(&scroll);

        (
            Self {
                empty_label,
                access_point_box,
                rows: HashMap::new(),
                on_command,
            },
            outer,
        )
    }

    pub fn update(
        &mut self,
        access_points: &[WifiAccessPoint],
        _wifi_enabled: bool,
        active_action: Option<&NetworkActiveAction>,
    ) {
        let mut visible: Vec<&WifiAccessPoint> = access_points
            .iter()
            .filter(|access_point| !access_point.ssid.is_empty())
            .collect();
        visible.sort_by(|left, right| {
            right
                .connected
                .cmp(&left.connected)
                .then(right.saved.cmp(&left.saved))
                .then(right.strength.cmp(&left.strength))
        });

        let visible_ssids: HashSet<&str> = visible
            .iter()
            .map(|access_point| access_point.ssid.as_str())
            .collect();
        let to_remove: Vec<String> = self
            .rows
            .keys()
            .filter(|ssid| !visible_ssids.contains(ssid.as_str()))
            .cloned()
            .collect();
        for ssid in to_remove {
            if let Some(row) = self.rows.remove(&ssid) {
                row.popover_menu.unparent();
                self.access_point_box.remove(&row.button);
            }
        }

        for (index, access_point) in visible.iter().enumerate() {
            if let Some(row) = self.rows.get(&access_point.ssid) {
                row.update(access_point, active_action);
                reorder(
                    &self.access_point_box,
                    &row.button,
                    &self.rows,
                    &visible,
                    index,
                    |ap| ap.ssid.as_str(),
                );
            } else {
                let row = WifiRow::new(access_point, active_action, self.on_command.clone());
                self.access_point_box.append(&row.button);
                reorder(
                    &self.access_point_box,
                    &row.button,
                    &self.rows,
                    &visible,
                    index,
                    |ap| ap.ssid.as_str(),
                );
                self.rows.insert(access_point.ssid.clone(), row);
            }
        }

        self.empty_label.set_visible(visible.is_empty());
    }
}

impl WifiRow {
    fn new(
        access_point: &WifiAccessPoint,
        active_action: Option<&NetworkActiveAction>,
        on_command: NetworkCommandSender,
    ) -> Self {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let icon = gtk::Image::from_icon_name(signal_icon_name(access_point.strength));
        icon.set_pixel_size(16);
        icon.set_valign(gtk::Align::Center);
        row.append(&icon);

        let name_label = gtk::Label::new(Some(&access_point.ssid));
        name_label.set_hexpand(true);
        name_label.set_halign(gtk::Align::Start);
        name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name_label.set_max_width_chars(25);
        row.append(&name_label);

        let right_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        right_box.set_valign(gtk::Align::Center);

        let lock_icon = gtk::Image::from_icon_name("channel-secure-symbolic");
        lock_icon.set_pixel_size(12);
        lock_icon.add_css_class("net-lock-icon");
        right_box.append(&lock_icon);

        let spinner = gtk::Spinner::new();
        spinner.set_size_request(16, 16);
        right_box.append(&spinner);
        row.append(&right_box);

        let button = gtk::Button::new();
        button.set_child(Some(&row));
        button.add_css_class("flat");
        button.add_css_class("net-ap-btn");

        let state = Rc::new(RefCell::new(access_point.clone()));
        {
            let state = state.clone();
            let on_command = on_command.clone();
            button.connect_clicked(move |_| {
                if let Some(command) = command_for_access_point_click(&state.borrow()) {
                    on_command(command);
                }
            });
        }

        let popover_menu = gtk::PopoverMenu::from_model(None::<&gtk::gio::MenuModel>);
        popover_menu.set_parent(&button);
        popover_menu.set_has_arrow(false);

        let actions = gtk::gio::SimpleActionGroup::new();
        {
            let state = state.clone();
            let on_command = on_command.clone();
            let action = gtk::gio::SimpleAction::new("forget", None);
            action.connect_activate(move |_, _| {
                if let Some(command) = forget_command_for_access_point(&state.borrow()) {
                    on_command(command);
                }
            });
            actions.add_action(&action);
        }
        button.insert_action_group("net", Some(&actions));

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        {
            let state = state.clone();
            let popover_menu_ref = popover_menu.clone();
            right_click.connect_pressed(move |gesture, _, _, _| {
                if has_forget_menu(&state.borrow()) {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    popover_menu_ref.popup();
                }
            });
        }
        button.add_controller(right_click);

        let row = Self {
            button,
            icon,
            name_label,
            lock_icon,
            spinner,
            popover_menu,
            state,
        };
        row.update(access_point, active_action);
        row
    }

    fn update(&self, access_point: &WifiAccessPoint, active_action: Option<&NetworkActiveAction>) {
        *self.state.borrow_mut() = access_point.clone();
        self.icon
            .set_icon_name(Some(signal_icon_name(access_point.strength)));
        self.name_label.set_label(&access_point.ssid);
        self.lock_icon.set_visible(
            !access_point.security.is_empty()
                && access_point.security != "open"
                && !access_point.connected,
        );
        apply_icon_style(&self.icon, access_point.connected);

        let action_active = matches_access_point_action(access_point, active_action);
        self.spinner.set_visible(action_active);
        if action_active {
            self.spinner.start();
        } else {
            self.spinner.stop();
        }

        if has_forget_menu(access_point) {
            let menu = forget_menu_model();
            self.popover_menu.set_menu_model(Some(&menu));
        } else {
            self.popover_menu
                .set_menu_model(None::<&gtk::gio::MenuModel>);
        }

        self.button
            .set_tooltip_text(Some(&access_point_tooltip(access_point)));
    }
}

fn forget_menu_model() -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Forget"), Some("net.forget"));
    menu
}

fn signal_icon_name(strength: u8) -> &'static str {
    if strength >= 80 {
        "network-wireless-signal-excellent-symbolic"
    } else if strength >= 55 {
        "network-wireless-signal-good-symbolic"
    } else if strength >= 30 {
        "network-wireless-signal-ok-symbolic"
    } else if strength > 0 {
        "network-wireless-signal-weak-symbolic"
    } else {
        "network-wireless-signal-none-symbolic"
    }
}

fn apply_icon_style(icon: &gtk::Image, connected: bool) {
    if connected {
        icon.remove_css_class("net-ap-icon");
        icon.add_css_class("net-ap-icon-active");
    } else {
        icon.remove_css_class("net-ap-icon-active");
        icon.add_css_class("net-ap-icon");
    }
}

fn access_point_tooltip(access_point: &WifiAccessPoint) -> String {
    let mut parts = Vec::new();
    if access_point.security != "open" {
        parts.push(access_point.security.to_uppercase());
    }
    if access_point.frequency > 0 {
        parts.push(band_label(access_point.frequency).to_string());
    }
    parts.push(format!("Signal: {}%", access_point.strength));
    parts.join(" · ")
}

fn band_label(frequency: u32) -> &'static str {
    if frequency < 3000 {
        "2.4 GHz"
    } else if frequency < 6000 {
        "5 GHz"
    } else {
        "6 GHz"
    }
}

fn matches_access_point_action(
    access_point: &WifiAccessPoint,
    active_action: Option<&NetworkActiveAction>,
) -> bool {
    match active_action {
        Some(NetworkActiveAction::ConnectWifi { ssid }) => ssid == &access_point.ssid,
        Some(NetworkActiveAction::ConnectSaved { uuid })
        | Some(NetworkActiveAction::Disconnect { uuid })
        | Some(NetworkActiveAction::Forget { uuid }) => access_point.uuid.as_ref() == Some(uuid),
        _ => false,
    }
}

fn command_for_access_point_click(access_point: &WifiAccessPoint) -> Option<NetworkCommand> {
    if access_point.connected {
        access_point
            .uuid
            .clone()
            .map(|uuid| NetworkCommand::Disconnect { uuid })
    } else if access_point.saved {
        access_point
            .uuid
            .clone()
            .map(|uuid| NetworkCommand::ConnectSaved { uuid })
    } else {
        Some(NetworkCommand::ConnectWifi {
            ssid: access_point.ssid.clone(),
        })
    }
}

fn forget_command_for_access_point(access_point: &WifiAccessPoint) -> Option<NetworkCommand> {
    access_point
        .saved
        .then_some(access_point.uuid.clone())
        .flatten()
        .map(|uuid| NetworkCommand::Forget { uuid })
}

fn has_forget_menu(access_point: &WifiAccessPoint) -> bool {
    access_point.saved && access_point.uuid.is_some()
}

fn reorder<'a, T>(
    parent: &gtk::Box,
    child: &gtk::Button,
    rows: &HashMap<String, WifiRow>,
    visible: &[&'a T],
    index: usize,
    key: impl Fn(&'a T) -> &'a str,
) {
    if index == 0 {
        parent.reorder_child_after(child, Option::<&gtk::Widget>::None);
    } else if let Some(previous) = visible.get(index - 1) {
        if let Some(previous_row) = rows.get(key(previous)) {
            parent.reorder_child_after(child, Some(&previous_row.button));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::applets::network::components::NetworkCommand;

    #[test]
    fn action_matching_uses_ssid_for_unsaved_networks_and_uuid_for_saved_networks() {
        let unsaved = WifiAccessPoint {
            ssid: "Cafe".into(),
            ..WifiAccessPoint::default()
        };
        assert!(matches_access_point_action(
            &unsaved,
            Some(&NetworkActiveAction::ConnectWifi {
                ssid: "Cafe".into(),
            })
        ));

        let saved = WifiAccessPoint {
            ssid: "Office".into(),
            uuid: Some("uuid-1".into()),
            ..WifiAccessPoint::default()
        };
        assert!(matches_access_point_action(
            &saved,
            Some(&NetworkActiveAction::ConnectSaved {
                uuid: "uuid-1".into(),
            })
        ));
    }

    #[test]
    fn click_command_prefers_current_saved_state_over_stale_connected_state() {
        let connected = WifiAccessPoint {
            ssid: "Office".into(),
            connected: true,
            uuid: Some("active-uuid".into()),
            ..WifiAccessPoint::default()
        };
        let saved = WifiAccessPoint {
            ssid: "Office".into(),
            saved: true,
            uuid: Some("saved-uuid".into()),
            ..WifiAccessPoint::default()
        };

        assert_eq!(
            command_for_access_point_click(&connected),
            Some(NetworkCommand::Disconnect {
                uuid: "active-uuid".into(),
            })
        );
        assert_eq!(
            command_for_access_point_click(&saved),
            Some(NetworkCommand::ConnectSaved {
                uuid: "saved-uuid".into(),
            })
        );
    }

    #[test]
    fn forget_command_requires_current_saved_uuid() {
        let unsaved = WifiAccessPoint {
            ssid: "Skylink".into(),
            ..WifiAccessPoint::default()
        };
        let saved = WifiAccessPoint {
            ssid: "Skylink".into(),
            saved: true,
            uuid: Some("saved-uuid".into()),
            ..WifiAccessPoint::default()
        };

        assert_eq!(forget_command_for_access_point(&unsaved), None);
        assert_eq!(
            forget_command_for_access_point(&saved),
            Some(NetworkCommand::Forget {
                uuid: "saved-uuid".into(),
            })
        );
    }

    #[test]
    fn forget_menu_appears_when_network_becomes_saved() {
        let unsaved = WifiAccessPoint {
            ssid: "Skylink".into(),
            ..WifiAccessPoint::default()
        };
        let saved = WifiAccessPoint {
            ssid: "Skylink".into(),
            saved: true,
            uuid: Some("saved-uuid".into()),
            ..WifiAccessPoint::default()
        };

        assert!(!has_forget_menu(&unsaved));
        assert!(has_forget_menu(&saved));
    }
}
