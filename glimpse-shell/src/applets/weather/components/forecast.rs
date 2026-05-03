#![allow(unused_assignments)]

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, prelude::*},
};

use crate::services::weather::model::{DailyForecast, State};

use super::super::format;

pub struct Forecast {
    items: Vec<DailyForecast>,
    root: gtk::Box,
}

#[derive(Debug)]
pub enum ForecastInput {
    Update(State),
}

struct ForecastRowInit {
    day_name: String,
    icon: String,
    condition: String,
    temperatures: String,
}

#[relm4::widget_template]
impl WidgetTemplate for ForecastRow {
    type Init = ForecastRowInit;

    view! {
        gtk::Box {
            add_css_class: "weather-forecast-row",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            set_valign: gtk::Align::Center,

            gtk::Label {
                add_css_class: "weather-forecast-day",
                set_label: &init.day_name,
                set_xalign: 0.0,
            },

            gtk::Label {
                add_css_class: "weather-forecast-temps",
                set_label: &init.temperatures,
            },

            gtk::Box {
                set_hexpand: true,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 6,
                set_valign: gtk::Align::Center,
                set_halign: gtk::Align::End,
                add_css_class: "weather-forecast-condition",

                gtk::Label {
                    add_css_class: "weather-forecast-cond",
                    set_label: &init.condition,
                    set_valign: gtk::Align::Center,
                    set_xalign: 1.0,
                    set_halign: gtk::Align::End,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_max_width_chars: 18,
                },

                gtk::Box {
                    add_css_class: "weather-forecast-icon-slot",
                    set_valign: gtk::Align::Center,
                    set_halign: gtk::Align::Center,

                    gtk::Image {
                        add_css_class: "weather-forecast-icon",
                        set_valign: gtk::Align::Center,
                        set_halign: gtk::Align::Center,
                        set_icon_name: Some(&init.icon),
                        set_pixel_size: 18,
                    },
                },
            },
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Forecast {
    type Init = ();
    type Input = ForecastInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Forecast {
            items: Vec::new(),
            root: root.clone(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            ForecastInput::Update(state) => {
                let items = match state {
                    State::Ready(snapshot) => forecast_items(snapshot.forecast),
                    State::Unknown | State::Loading | State::Unavailable(_) => Vec::new(),
                };
                self.update_items(items);
            }
        }
    }
}

impl Forecast {
    fn update_items(&mut self, items: Vec<DailyForecast>) {
        if self.items == items {
            return;
        }

        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }
        for item in &items {
            let row = forecast_row(item);
            self.root.append(row.as_ref());
        }
        self.root.set_visible(!items.is_empty());
        self.items = items;
    }
}

fn forecast_row(item: &DailyForecast) -> ForecastRow {
    let row = ForecastRow::init(ForecastRowInit {
        day_name: item.day_name.clone(),
        icon: item.icon.clone(),
        condition: item.condition.clone(),
        temperatures: format!(
            "{} / {}",
            format::temperature(item.temperature_min),
            format::temperature(item.temperature_max)
        ),
    });
    if item.is_today {
        row.as_ref().add_css_class("weather-forecast-today");
    }
    row
}

pub(in crate::applets::weather) fn has_forecast_items(items: &[DailyForecast]) -> bool {
    items.iter().any(|item| !item.is_today)
}

fn forecast_items(items: Vec<DailyForecast>) -> Vec<DailyForecast> {
    items.into_iter().filter(|item| !item.is_today).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forecast_items_start_after_today() {
        let today = DailyForecast {
            date: "2099-01-01".into(),
            day_name: "Today".into(),
            is_today: true,
            ..DailyForecast::default()
        };
        let tomorrow = DailyForecast {
            date: "2099-01-02".into(),
            day_name: "Fri".into(),
            ..DailyForecast::default()
        };

        let items = forecast_items(vec![today, tomorrow.clone()]);

        assert_eq!(items, vec![tomorrow]);
    }

    #[test]
    fn has_forecast_items_ignores_today() {
        assert!(!has_forecast_items(&[DailyForecast {
            is_today: true,
            ..DailyForecast::default()
        }]));
        assert!(has_forecast_items(&[DailyForecast {
            is_today: false,
            ..DailyForecast::default()
        }]));
    }
}
