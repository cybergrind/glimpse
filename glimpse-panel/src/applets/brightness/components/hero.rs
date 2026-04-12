use glimpse::providers::brightness::BrightnessDisplay;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::super::applet::applet_icon_name;

pub struct BrightnessHero {
    icon_name: String,
    subtitle: String,
}

#[derive(Debug)]
pub enum BrightnessHeroInput {
    Update(Option<BrightnessDisplay>),
}

#[relm4::component(pub)]
impl SimpleComponent for BrightnessHero {
    type Init = ();
    type Input = BrightnessHeroInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 12,
            add_css_class: "brightness-hero",

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 32,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_valign: gtk::Align::Center,

                gtk::Label {
                    set_label: "Brightness",
                    set_halign: gtk::Align::Start,
                    add_css_class: "brightness-hero-title",
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.subtitle,
                    set_halign: gtk::Align::Start,
                    add_css_class: "brightness-hero-subtitle",
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = BrightnessHero {
            icon_name: "display-brightness-off-symbolic".into(),
            subtitle: "No controllable displays".into(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            BrightnessHeroInput::Update(Some(display)) => {
                self.icon_name = applet_icon_name().into();
                self.subtitle = format!("{} • {}%", display.name, display.percentage);
            }
            BrightnessHeroInput::Update(None) => {
                self.icon_name = applet_icon_name().into();
                self.subtitle = "No controllable displays".into();
            }
        }
    }
}
