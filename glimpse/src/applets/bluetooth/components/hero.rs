#![allow(unused_assignments)]

use std::cell::Cell;
use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

pub struct BluetoothHero {
    icon_name: String,
    subtitle: String,
    powered: bool,
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
            set_spacing: 12,
            add_css_class: "bt-hero",

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 32,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,
                set_valign: gtk::Align::Center,

                gtk::Label {
                    set_label: "Bluetooth",
                    set_halign: gtk::Align::Start,
                    add_css_class: "bt-title",
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.subtitle,
                    set_halign: gtk::Align::Start,
                    add_css_class: "bt-subtitle",
                },
            },

            #[name(power_switch)]
            gtk::Switch {
                set_valign: gtk::Align::Center,
                set_tooltip_text: Some("Toggle all adapters"),
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = BluetoothHero {
            icon_name: "bluetooth-disabled-symbolic".into(),
            subtitle: "Off".into(),
            powered: false,
            power_switch: gtk::Switch::new(),
            updating_power: Rc::new(Cell::new(false)),
        };
        let widgets = view_output!();
        let mut model = model;
        model.power_switch = widgets.power_switch.clone();

        let guard = model.updating_power.clone();
        let sender = _sender.clone();
        widgets.power_switch.connect_state_set(move |_, active| {
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

        self.powered = powered;
        if self.power_switch.is_active() != powered {
            self.updating_power.set(true);
            self.power_switch.set_active(powered);
            self.power_switch.set_state(powered);
            self.updating_power.set(false);
        }
        self.icon_name = if powered {
            "bluetooth-active-symbolic"
        } else {
            "bluetooth-disabled-symbolic"
        }
        .into();
        self.subtitle = hero_subtitle_text(powered, discovering, connected_count, activity.as_deref());
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
