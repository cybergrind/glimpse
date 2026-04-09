use std::cell::Cell;
use std::rc::Rc;

use relm4::gtk::{self, glib, prelude::*};

use super::{BluetoothCommand, BluetoothCommandSender};

pub struct BluetoothHero {
    icon: gtk::Image,
    subtitle: gtk::Label,
    power_switch: gtk::Switch,
    updating_power: Rc<Cell<bool>>,
    powered: bool,
    discovering: bool,
    connected_count: u32,
    activity: Option<String>,
}

impl BluetoothHero {
    pub fn new(on_command: BluetoothCommandSender) -> (Self, gtk::Box) {
        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hero.add_css_class("bt-hero");

        let icon = gtk::Image::from_icon_name("bluetooth-active-symbolic");
        icon.set_pixel_size(32);
        hero.append(&icon);

        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        title_box.set_hexpand(true);
        title_box.set_valign(gtk::Align::Center);

        let title = gtk::Label::new(Some("Bluetooth"));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("bt-title");
        title_box.append(&title);

        let subtitle = gtk::Label::new(Some("Off"));
        subtitle.set_halign(gtk::Align::Start);
        subtitle.add_css_class("bt-subtitle");
        title_box.append(&subtitle);

        hero.append(&title_box);

        let power_switch = gtk::Switch::new();
        power_switch.set_valign(gtk::Align::Center);
        power_switch.set_tooltip_text(Some("Toggle all adapters"));

        let updating_power = Rc::new(Cell::new(false));
        let guard = updating_power.clone();
        power_switch.connect_state_set(move |_, active| {
            if guard.get() {
                return glib::Propagation::Stop;
            }
            tracing::info!(powered = active, "bluetooth ui: power toggle clicked");
            on_command(BluetoothCommand::SetPowered(active));
            glib::Propagation::Stop
        });
        hero.append(&power_switch);

        let model = Self {
            icon,
            subtitle,
            power_switch,
            updating_power,
            powered: false,
            discovering: false,
            connected_count: 0,
            activity: None,
        };

        (model, hero)
    }

    pub fn update_status(&mut self, powered: bool, discovering: bool) {
        self.powered = powered;
        self.discovering = discovering;

        if self.power_switch.is_active() != powered {
            self.updating_power.set(true);
            self.power_switch.set_active(powered);
            self.power_switch.set_state(powered);
            self.updating_power.set(false);
        }

        self.icon.set_icon_name(Some(if powered {
            "bluetooth-active-symbolic"
        } else {
            "bluetooth-disabled-symbolic"
        }));

        self.refresh_subtitle();
    }

    pub fn update_connected_count(&mut self, count: u32) {
        self.connected_count = count;
        self.refresh_subtitle();
    }

    pub fn set_activity(&mut self, activity: Option<String>) {
        self.activity = activity;
        self.refresh_subtitle();
    }

    fn refresh_subtitle(&self) {
        let text = hero_subtitle_text(
            self.powered,
            self.discovering,
            self.connected_count,
            self.activity.as_deref(),
        );
        self.subtitle.set_label(&text);
    }
}

fn hero_subtitle_text(
    powered: bool,
    discovering: bool,
    connected_count: u32,
    activity: Option<&str>,
) -> String {
    if let Some(activity) = activity {
        return activity.to_owned();
    }

    if !powered {
        "Off".into()
    } else if discovering {
        "Discovering".into()
    } else if connected_count > 0 {
        format!("{connected_count} connected")
    } else {
        "Ready".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_prefers_activity_then_discovery_then_connection_state() {
        assert_eq!(
            hero_subtitle_text(true, true, 2, Some("Pairing Headphones...")),
            "Pairing Headphones..."
        );
        assert_eq!(hero_subtitle_text(true, true, 2, None), "Discovering");
        assert_eq!(hero_subtitle_text(true, false, 2, None), "2 connected");
        assert_eq!(hero_subtitle_text(true, false, 0, None), "Ready");
        assert_eq!(hero_subtitle_text(false, true, 2, None), "Off");
    }
}
