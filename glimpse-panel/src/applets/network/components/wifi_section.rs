#![allow(unused_assignments)]

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use glimpse::network::protocol::NetworkActiveAction;
use glimpse::network::provider::WifiAccessPoint;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::{NetworkAction, NetworkActionSender};

pub struct WifiSection {
    empty_label: gtk::Label,
    access_point_box: gtk::Box,
    rows: HashMap<String, WifiRow>,
    on_action: NetworkActionSender,
}

#[derive(Debug)]
pub enum WifiSectionInput {
    Update {
        access_points: Vec<WifiAccessPoint>,
        wifi_enabled: bool,
        active_action: Option<NetworkActiveAction>,
    },
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

#[relm4::component(pub)]
impl SimpleComponent for WifiSection {
    type Init = ();
    type Input = WifiSectionInput;
    type Output = NetworkAction;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,

            #[name(empty_label)]
            gtk::Label {
                set_label: "No access points",
                set_halign: gtk::Align::Start,
                add_css_class: "net-empty",
            },

            gtk::ScrolledWindow {
                set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                set_max_content_height: 300,
                set_propagate_natural_height: true,

                #[name(access_point_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },
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

        let model = WifiSection {
            empty_label: gtk::Label::new(None),
            access_point_box: gtk::Box::new(gtk::Orientation::Vertical, 0),
            rows: HashMap::new(),
            on_action,
        };
        let widgets = view_output!();

        let mut model = model;
        model.empty_label = widgets.empty_label.clone();
        model.access_point_box = widgets.access_point_box.clone();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let WifiSectionInput::Update {
            access_points,
            wifi_enabled,
            active_action,
        } = message;

        let _ = wifi_enabled;

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

        let visible_paths: HashSet<&str> = visible
            .iter()
            .map(|access_point| access_point.path.as_str())
            .collect();
        let to_remove: Vec<String> = self
            .rows
            .keys()
            .filter(|path| !visible_paths.contains(path.as_str()))
            .cloned()
            .collect();
        for path in to_remove {
            if let Some(row) = self.rows.remove(&path) {
                row.popover_menu.unparent();
                self.access_point_box.remove(&row.button);
            }
        }

        for (index, access_point) in visible.iter().enumerate() {
            if let Some(row) = self.rows.get(&access_point.path) {
                row.update(access_point, active_action.as_ref());
                reorder(
                    &self.access_point_box,
                    &row.button,
                    &self.rows,
                    &visible,
                    index,
                    |ap| ap.path.as_str(),
                );
            } else {
                let row =
                    WifiRow::new(access_point, active_action.as_ref(), self.on_action.clone());
                self.access_point_box.append(&row.button);
                reorder(
                    &self.access_point_box,
                    &row.button,
                    &self.rows,
                    &visible,
                    index,
                    |ap| ap.path.as_str(),
                );
                self.rows.insert(access_point.path.clone(), row);
            }
        }

        self.empty_label.set_visible(visible.is_empty());
    }
}

impl WifiRow {
    fn new(
        access_point: &WifiAccessPoint,
        active_action: Option<&NetworkActiveAction>,
        on_action: NetworkActionSender,
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
            let on_action = on_action.clone();
            button.connect_clicked(move |_| {
                if let Some(action) = action_for_access_point_click(&state.borrow()) {
                    on_action(action);
                }
            });
        }

        let popover_menu = gtk::PopoverMenu::from_model(None::<&gtk::gio::MenuModel>);
        popover_menu.set_parent(&button);
        popover_menu.set_has_arrow(false);

        let actions = gtk::gio::SimpleActionGroup::new();
        {
            let state = state.clone();
            let on_action = on_action.clone();
            let action = gtk::gio::SimpleAction::new("forget", None);
            action.connect_activate(move |_, _| {
                if let Some(action) = forget_action_for_access_point(&state.borrow()) {
                    on_action(action);
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
        Some(NetworkActiveAction::ConnectWifi { path, .. }) => path == &access_point.path,
        Some(NetworkActiveAction::ConnectSaved { uuid })
        | Some(NetworkActiveAction::Disconnect { uuid })
        | Some(NetworkActiveAction::Forget { uuid }) => access_point.uuid.as_ref() == Some(uuid),
        _ => false,
    }
}

fn action_for_access_point_click(access_point: &WifiAccessPoint) -> Option<NetworkAction> {
    if access_point.connected {
        access_point
            .uuid
            .clone()
            .map(|uuid| NetworkAction::Disconnect { uuid })
    } else if access_point.saved {
        access_point
            .uuid
            .clone()
            .map(|uuid| NetworkAction::ConnectSaved { uuid })
    } else {
        Some(NetworkAction::ConnectWifi {
            ssid: access_point.ssid.clone(),
            path: access_point.path.clone(),
        })
    }
}

fn forget_action_for_access_point(access_point: &WifiAccessPoint) -> Option<NetworkAction> {
    access_point
        .saved
        .then_some(access_point.uuid.clone())
        .flatten()
        .map(|uuid| NetworkAction::Forget { uuid })
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
    use crate::applets::network::components::NetworkAction;

    #[test]
    fn action_matching_uses_ssid_for_unsaved_networks_and_uuid_for_saved_networks() {
        let unsaved = WifiAccessPoint {
            path: "/ap/1".into(),
            ssid: "Cafe".into(),
            ..WifiAccessPoint::default()
        };
        assert!(matches_access_point_action(
            &unsaved,
            Some(&NetworkActiveAction::ConnectWifi {
                ssid: "Cafe".into(),
                path: "/ap/1".into(),
            })
        ));

        let saved = WifiAccessPoint {
            path: "/ap/2".into(),
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
    fn click_action_prefers_current_saved_state_over_stale_connected_state() {
        let connected = WifiAccessPoint {
            path: "/ap/1".into(),
            ssid: "Office".into(),
            connected: true,
            uuid: Some("active-uuid".into()),
            ..WifiAccessPoint::default()
        };
        let saved = WifiAccessPoint {
            path: "/ap/2".into(),
            ssid: "Office".into(),
            saved: true,
            uuid: Some("saved-uuid".into()),
            ..WifiAccessPoint::default()
        };

        assert_eq!(
            action_for_access_point_click(&connected),
            Some(NetworkAction::Disconnect {
                uuid: "active-uuid".into(),
            })
        );
        assert_eq!(
            action_for_access_point_click(&saved),
            Some(NetworkAction::ConnectSaved {
                uuid: "saved-uuid".into(),
            })
        );
    }

    #[test]
    fn forget_action_requires_current_saved_uuid() {
        let unsaved = WifiAccessPoint {
            path: "/ap/1".into(),
            ssid: "Skylink".into(),
            ..WifiAccessPoint::default()
        };
        let saved = WifiAccessPoint {
            path: "/ap/2".into(),
            ssid: "Skylink".into(),
            saved: true,
            uuid: Some("saved-uuid".into()),
            ..WifiAccessPoint::default()
        };

        assert_eq!(forget_action_for_access_point(&unsaved), None);
        assert_eq!(
            forget_action_for_access_point(&saved),
            Some(NetworkAction::Forget {
                uuid: "saved-uuid".into(),
            })
        );
    }

    #[test]
    fn forget_menu_appears_when_network_becomes_saved() {
        let unsaved = WifiAccessPoint {
            path: "/ap/1".into(),
            ssid: "Skylink".into(),
            ..WifiAccessPoint::default()
        };
        let saved = WifiAccessPoint {
            path: "/ap/2".into(),
            ssid: "Skylink".into(),
            saved: true,
            uuid: Some("saved-uuid".into()),
            ..WifiAccessPoint::default()
        };

        assert!(!has_forget_menu(&unsaved));
        assert!(has_forget_menu(&saved));
    }
}
