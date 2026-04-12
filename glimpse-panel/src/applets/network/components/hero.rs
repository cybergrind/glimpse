use std::{cell::Cell, rc::Rc};

use glimpse::network::provider::NetworkStatus;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use super::NetworkAction;

pub struct NetworkHero {
    icon_name: String,
    subtitle: String,
    wifi_enabled: bool,
    updating_switch: Rc<Cell<bool>>,
    wifi_switch: gtk::Switch,
}

#[derive(Debug)]
pub enum NetworkHeroInput {
    Update {
        status: NetworkStatus,
        scanning: bool,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for NetworkHero {
    type Init = ();
    type Input = NetworkHeroInput;
    type Output = NetworkAction;

    view! {
        gtk::Box {
            set_spacing: 12,
            add_css_class: "net-hero",

            #[name(icon)]
            gtk::Image {
                set_pixel_size: 32,
                #[watch]
                set_icon_name: Some(&model.icon_name),
            },

            gtk::Box {
                set_hexpand: true,
                set_valign: gtk::Align::Center,
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,

                gtk::Label {
                    set_halign: gtk::Align::Start,
                    add_css_class: "net-title",
                    set_label: "Network",
                },

                #[name(subtitle_label)]
                gtk::Label {
                    set_halign: gtk::Align::Start,
                    add_css_class: "net-subtitle",
                    #[watch]
                    set_label: &model.subtitle,
                },
            },

            #[name(wifi_switch)]
            gtk::Switch {
                set_valign: gtk::Align::Center,
                set_tooltip_text: Some("Toggle WiFi"),
                #[watch]
                set_active: model.wifi_enabled,
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = NetworkHero {
            icon_name: "network-offline-symbolic".into(),
            subtitle: "Offline".into(),
            wifi_enabled: false,
            updating_switch: Rc::new(Cell::new(false)),
            wifi_switch: gtk::Switch::new(),
        };
        let widgets = view_output!();
        let mut model = model;
        model.wifi_switch = widgets.wifi_switch.clone();

        let guard = model.updating_switch.clone();
        let sender = sender.clone();
        widgets.wifi_switch.connect_state_set(move |_, active| {
            if guard.get() {
                return glib::Propagation::Stop;
            }
            let _ = sender.output(NetworkAction::ToggleWifi(active));
            glib::Propagation::Stop
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let NetworkHeroInput::Update { status, scanning } = message;

        self.icon_name = status.icon.clone();
        self.subtitle = hero_subtitle_text(&status, scanning);

        if self.wifi_switch.is_active() != status.wifi_enabled {
            self.updating_switch.set(true);
            self.wifi_enabled = status.wifi_enabled;
            self.wifi_switch.set_active(status.wifi_enabled);
            self.wifi_switch.set_state(status.wifi_enabled);
            self.updating_switch.set(false);
        } else {
            self.wifi_enabled = status.wifi_enabled;
        }
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
