use std::cell::Cell;
use std::rc::Rc;

use relm4::gtk::{self, glib, prelude::*};

use super::NotificationCommandEmitter;
use crate::applets::notifications::NotificationActionCommand;

pub struct NotificationsHero {
    root: gtk::Box,
    icon: gtk::Image,
    subtitle: gtk::Label,
    dnd_switch: gtk::Switch,
    updating_dnd: Rc<Cell<bool>>,
}

impl NotificationsHero {
    pub fn new(emit_command: NotificationCommandEmitter) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        root.add_css_class("notif-hero");

        let icon = gtk::Image::from_icon_name("preferences-system-notifications-symbolic");
        icon.set_pixel_size(32);
        root.append(&icon);

        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text_box.set_hexpand(true);
        text_box.set_valign(gtk::Align::Center);

        let title = gtk::Label::new(Some("Notifications"));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("notif-title");
        text_box.append(&title);

        let subtitle = gtk::Label::new(Some("No notifications"));
        subtitle.set_halign(gtk::Align::Start);
        subtitle.add_css_class("notif-subtitle");
        text_box.append(&subtitle);
        root.append(&text_box);

        let dnd_switch = gtk::Switch::new();
        dnd_switch.set_active(true);
        dnd_switch.set_valign(gtk::Align::Center);
        dnd_switch.set_tooltip_text(Some("Notifications"));
        let updating_dnd = Rc::new(Cell::new(false));
        let guard = updating_dnd.clone();
        dnd_switch.connect_state_set(move |_, active| {
            if guard.get() {
                return glib::Propagation::Stop;
            }
            emit_command(NotificationActionCommand::SetDnd(!active));
            glib::Propagation::Stop
        });
        root.append(&dnd_switch);

        Self {
            root,
            icon,
            subtitle,
            dnd_switch,
            updating_dnd,
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn update_status(&self, dnd: bool, count: u32) {
        let switch_active = !dnd;
        if self.dnd_switch.is_active() != switch_active {
            self.updating_dnd.set(true);
            self.dnd_switch.set_active(switch_active);
            self.dnd_switch.set_state(switch_active);
            self.updating_dnd.set(false);
        }

        self.icon.set_icon_name(Some(if dnd {
            "notifications-disabled-symbolic"
        } else {
            "preferences-system-notifications-symbolic"
        }));

        let subtitle = if count == 0 {
            "No notifications".to_string()
        } else if count == 1 {
            "1 notification".to_string()
        } else {
            format!("{count} notifications")
        };
        self.subtitle.set_label(&subtitle);
    }
}
