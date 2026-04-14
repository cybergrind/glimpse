#![allow(unused_assignments)]

use std::cell::Cell;
use std::rc::Rc;

use relm4::{
    Component, ComponentController, Controller,
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use crate::components::hero_row::{HeroRow, HeroRowInit, HeroRowInput};

pub struct BluetoothHero {
    row: Controller<HeroRow>,
    icon: gtk::Image,
    power_switch: gtk::Switch,
    updating_power: Rc<Cell<bool>>,
}

#[derive(Debug)]
pub enum BluetoothHeroInput {
    Update {
        powered: bool,
        discovering: bool,
        connected_count: u32,
        activity: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothHeroOutput {
    SetPowered(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for BluetoothHero {
    type Init = ();
    type Input = BluetoothHeroInput;
    type Output = BluetoothHeroOutput;

    view! {
        gtk::Box {
            #[local_ref]
            row_widget -> gtk::Box {}
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let row = HeroRow::builder()
            .launch(HeroRowInit {
                title: "Bluetooth".into(),
                subtitle: "Off".into(),
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

        let icon = gtk::Image::from_icon_name("bluetooth-disabled-symbolic");
        icon.set_pixel_size(32);
        media.append(&icon);

        let power_switch = gtk::Switch::new();
        power_switch.set_valign(gtk::Align::Center);
        power_switch.set_tooltip_text(Some("Toggle all adapters"));
        trailing.append(&power_switch);

        let model = BluetoothHero {
            row,
            icon,
            power_switch,
            updating_power: Rc::new(Cell::new(false)),
        };
        let widgets = view_output!();
        let guard = model.updating_power.clone();
        let sender = _sender.clone();
        model.power_switch.connect_state_set(move |_, active| {
            if guard.get() {
                return glib::Propagation::Stop;
            }
            tracing::info!(powered = active, "bluetooth ui: power toggle clicked");
            let _ = sender.output(BluetoothHeroOutput::SetPowered(active));
            glib::Propagation::Stop
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let BluetoothHeroInput::Update {
            powered,
            discovering,
            connected_count,
            activity,
        } = msg;

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
        self.row.emit(HeroRowInput::Update {
            title: "Bluetooth".into(),
            subtitle: hero_subtitle_text(powered, discovering, connected_count, activity.as_deref()),
        });
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
