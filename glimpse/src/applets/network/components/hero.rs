use std::{cell::Cell, rc::Rc};

use glimpse::network::provider::NetworkStatus;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use super::NetworkAction;
use crate::components::hero_row::{HeroRow, HeroRowInit, HeroRowInput};

pub struct NetworkHero {
    row: Controller<HeroRow>,
    icon: gtk::Image,
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
            #[local_ref]
            row_widget -> gtk::Box {}
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let row = HeroRow::builder()
            .launch(HeroRowInit {
                title: "Network".into(),
                subtitle: "Offline".into(),
            })
            .detach();
        let row_widget = row.widget().clone();
        let media = row_widget
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("hero row should expose media slot");
        let content = media
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("hero row should expose content slot");
        let trailing = content
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("hero row should expose trailing slot");

        let icon = gtk::Image::from_icon_name("network-offline-symbolic");
        icon.set_pixel_size(32);
        media.append(&icon);

        let wifi_switch = gtk::Switch::new();
        wifi_switch.set_valign(gtk::Align::Center);
        wifi_switch.set_tooltip_text(Some("Toggle WiFi"));
        trailing.append(&wifi_switch);

        let model = NetworkHero {
            row,
            icon,
            wifi_enabled: false,
            updating_switch: Rc::new(Cell::new(false)),
            wifi_switch,
        };
        let widgets = view_output!();

        let guard = model.updating_switch.clone();
        let sender = sender.clone();
        model.wifi_switch.connect_state_set(move |_, active| {
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
        self.icon.set_icon_name(Some(&status.icon));
        self.row.emit(HeroRowInput::Update {
            title: "Network".into(),
            subtitle: hero_subtitle_text(&status, scanning),
        });

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
