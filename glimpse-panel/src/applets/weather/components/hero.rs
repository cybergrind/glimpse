use relm4::{
    gtk::{self, prelude::*},
    ComponentParts, ComponentSender, SimpleComponent,
};

use super::super::applet::{WeatherCurrent, WeatherLocation};

pub struct WeatherHero {
    icon_name: String,
    temperature: String,
    summary: String,
    location: String,
}

#[derive(Debug)]
pub enum WeatherHeroInput {
    UpdateSnapshot {
        current: WeatherCurrent,
        location: WeatherLocation,
    },
    Clear,
}

#[relm4::component(pub)]
impl SimpleComponent for WeatherHero {
    type Init = ();
    type Input = WeatherHeroInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "weather-hero",

                gtk::Image {
                    #[watch]
                    set_icon_name: Some(&model.icon_name),
                    set_pixel_size: 32,
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.temperature,
                    add_css_class: "weather-hero-temp",
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.location,
                    set_halign: gtk::Align::End,
                    set_hexpand: true,
                    set_ellipsize: hero_location_constraints().1,
                    set_max_width_chars: hero_location_constraints().0,
                    add_css_class: "weather-hero-location",
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                add_css_class: "weather-hero-row2",

                gtk::Label {
                    #[watch]
                    set_label: &model.summary,
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                    add_css_class: "weather-hero-condition",
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = WeatherHero {
            icon_name: "weather-overcast-symbolic".into(),
            temperature: "—".into(),
            summary: String::new(),
            location: String::new(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            WeatherHeroInput::UpdateSnapshot { current, location } => {
                self.summary = hero_summary(&current);
                self.icon_name = current.icon;
                self.temperature = format!("{:.0}°", current.temperature);
                self.location = location.city;
            }
            WeatherHeroInput::Clear => {
                self.icon_name = "weather-overcast-symbolic".into();
                self.temperature = "—".into();
                self.summary.clear();
                self.location.clear();
            }
        }
    }
}

pub fn hero_summary(current: &WeatherCurrent) -> String {
    format!(
        "{} · Feels like {:.0}°",
        current.condition, current.apparent_temperature
    )
}

pub fn hero_location_constraints() -> (i32, gtk::pango::EllipsizeMode) {
    (24, gtk::pango::EllipsizeMode::End)
}
