use std::cell::Cell;
use std::rc::Rc;

use glimpse::providers::network::NetworkStatus;
use relm4::gtk::{self, glib, prelude::*};

use super::{NetworkCommand, NetworkCommandSender};

pub struct NetworkHero {
    icon: gtk::Image,
    subtitle: gtk::Label,
    wifi_switch: gtk::Switch,
    updating_switch: Rc<Cell<bool>>,
}

impl NetworkHero {
    pub fn new(on_command: NetworkCommandSender) -> (Self, gtk::Box) {
        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hero.add_css_class("net-hero");

        let icon = gtk::Image::from_icon_name("network-offline-symbolic");
        icon.set_pixel_size(32);
        hero.append(&icon);

        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        title_box.set_hexpand(true);
        title_box.set_valign(gtk::Align::Center);

        let title = gtk::Label::new(Some("Network"));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("net-title");
        title_box.append(&title);

        let subtitle = gtk::Label::new(Some("Offline"));
        subtitle.set_halign(gtk::Align::Start);
        subtitle.add_css_class("net-subtitle");
        title_box.append(&subtitle);
        hero.append(&title_box);

        let wifi_switch = gtk::Switch::new();
        wifi_switch.set_valign(gtk::Align::Center);
        wifi_switch.set_tooltip_text(Some("Toggle WiFi"));

        let updating_switch = Rc::new(Cell::new(false));
        let guard = updating_switch.clone();
        wifi_switch.connect_state_set(move |_, active| {
            if guard.get() {
                return glib::Propagation::Stop;
            }
            on_command(NetworkCommand::ToggleWifi(active));
            glib::Propagation::Stop
        });
        hero.append(&wifi_switch);

        (
            Self {
                icon,
                subtitle,
                wifi_switch,
                updating_switch,
            },
            hero,
        )
    }

    pub fn update(&mut self, status: &NetworkStatus, scanning: bool) {
        self.icon.set_icon_name(Some(&status.icon));
        if self.wifi_switch.is_active() != status.wifi_enabled {
            self.updating_switch.set(true);
            self.wifi_switch.set_active(status.wifi_enabled);
            self.wifi_switch.set_state(status.wifi_enabled);
            self.updating_switch.set(false);
        }
        self.subtitle
            .set_label(&hero_subtitle_text(status, scanning));
    }
}

fn hero_subtitle_text(status: &NetworkStatus, scanning: bool) -> String {
    if scanning {
        return "Scanning…".into();
    }
    if status.primary_connection.is_empty() {
        "Offline".into()
    } else {
        let mut parts = vec![status.primary_connection.clone()];
        if status.metered {
            parts.push("Metered".into());
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_uses_primary_connection_and_metered_suffix() {
        let mut status = NetworkStatus::default();
        assert_eq!(hero_subtitle_text(&status, false), "Offline");

        status.primary_connection = "Home".into();
        assert_eq!(hero_subtitle_text(&status, false), "Home");

        status.metered = true;
        assert_eq!(hero_subtitle_text(&status, false), "Home · Metered");
    }

    #[test]
    fn subtitle_prefers_scanning_status() {
        let mut status = NetworkStatus::default();
        status.primary_connection = "Home".into();

        assert_eq!(hero_subtitle_text(&status, true), "Scanning…");
    }
}
