#![allow(unused_assignments)]

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, prelude::*},
};

use crate::services::weather::model::{HourlyForecast, State};

use super::super::format;

pub struct Hourly {
    items: Vec<HourlyForecast>,
    root: gtk::Box,
}

#[derive(Debug)]
pub enum HourlyInput {
    Update(State),
}

struct HourlyColumnInit {
    time: String,
    icon: String,
    temperature: String,
}

#[relm4::widget_template]
impl WidgetTemplate for HourlyColumn {
    type Init = HourlyColumnInit;

    view! {
        gtk::Box {
            add_css_class: "weather-hourly-col",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 4,
            set_hexpand: true,

            gtk::Label {
                add_css_class: "weather-hourly-time",
                set_label: &init.time,
            },

            gtk::Image {
                set_icon_name: Some(&init.icon),
                set_pixel_size: 20,
            },

            gtk::Label {
                add_css_class: "weather-hourly-temp",
                set_label: &init.temperature,
            },
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Hourly {
    type Init = ();
    type Input = HourlyInput;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "weather-hourly",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            set_hexpand: true,
            set_homogeneous: true,
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Hourly {
            items: Vec::new(),
            root: root.clone(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            HourlyInput::Update(state) => {
                let items = match state {
                    State::Ready(snapshot) => snapshot.hourly,
                    State::Unknown | State::Loading | State::Unavailable(_) => Vec::new(),
                };
                self.update_items(items);
            }
        }
    }
}

impl Hourly {
    fn update_items(&mut self, items: Vec<HourlyForecast>) {
        if self.items == items {
            return;
        }

        while let Some(child) = self.root.first_child() {
            self.root.remove(&child);
        }
        for item in &items {
            let column = hourly_column(item);
            self.root.append(column.as_ref());
        }
        self.root.set_visible(!items.is_empty());
        self.items = items;
    }
}

fn hourly_column(item: &HourlyForecast) -> HourlyColumn {
    HourlyColumn::init(HourlyColumnInit {
        time: item.time.clone(),
        icon: item.icon.clone(),
        temperature: format::temperature(item.temperature),
    })
}
