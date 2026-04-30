use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::services::weather::model::{CurrentWeather, State};

use super::super::format;

pub struct Hero {
    icon_name: String,
    temperature: String,
    location: String,
    summary: String,
}

#[derive(Debug)]
pub enum HeroInput {
    Update(State),
}

#[relm4::component(pub)]
impl SimpleComponent for Hero {
    type Init = ();
    type Input = HeroInput;
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
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_max_width_chars: 24,
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
                    set_xalign: 0.0,
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
        let model = Hero {
            icon_name: "weather-overcast-symbolic".into(),
            temperature: "—".into(),
            location: String::new(),
            summary: "Weather unavailable".into(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            HeroInput::Update(state) => self.apply_state(&state),
        }
    }
}

impl Hero {
    fn apply_state(&mut self, state: &State) {
        match state {
            State::Ready(snapshot) => {
                self.icon_name = snapshot.current.icon.clone();
                self.temperature = format::temperature(snapshot.current.temperature);
                self.location = snapshot.location.city.clone();
                self.summary = hero_summary(&snapshot.current);
            }
            State::Loading => {
                self.icon_name = "weather-overcast-symbolic".into();
                self.temperature = "—".into();
                self.location.clear();
                self.summary = "Loading weather".into();
            }
            State::Unknown => {
                self.icon_name = "weather-overcast-symbolic".into();
                self.temperature = "—".into();
                self.location.clear();
                self.summary = "Weather unavailable".into();
            }
            State::Unavailable(message) => {
                self.icon_name = "weather-overcast-symbolic".into();
                self.temperature = "—".into();
                self.location.clear();
                self.summary = if message.is_empty() {
                    "Weather unavailable".into()
                } else {
                    message.clone()
                };
            }
        }
    }
}

pub fn hero_summary(current: &CurrentWeather) -> String {
    format!(
        "{} · Feels like {}",
        current.condition,
        format::temperature(current.apparent_temperature)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hero_summary_formats_condition_and_feels_like() {
        let current = CurrentWeather {
            condition: "Overcast".into(),
            apparent_temperature: 9.0,
            ..CurrentWeather::default()
        };

        assert_eq!(hero_summary(&current), "Overcast · Feels like 9°");
    }
}
