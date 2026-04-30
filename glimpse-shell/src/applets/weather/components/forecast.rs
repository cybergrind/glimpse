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

            gtk::Label {
                add_css_class: "weather-forecast-day",
                set_label: &init.day_name,
                set_xalign: 0.0,
                set_hexpand: true,
            },

            gtk::Image {
                set_icon_name: Some(&init.icon),
                set_pixel_size: 20,
            },

            gtk::Label {
                add_css_class: "weather-forecast-cond",
                set_label: &init.condition,
                set_xalign: 0.0,
                set_hexpand: true,
            },

            gtk::Label {
                add_css_class: "weather-forecast-temps",
                set_label: &init.temperatures,
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
                    State::Ready(snapshot) => snapshot.forecast,
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
            format::temperature(item.temperature_max),
            format::temperature(item.temperature_min)
        ),
    });
    if item.is_today {
        row.as_ref().add_css_class("weather-forecast-today");
    }
    row
}
