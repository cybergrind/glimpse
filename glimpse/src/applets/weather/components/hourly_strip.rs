use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::super::applet::WeatherHourly;

pub struct WeatherHourlyStrip {
    hourly_slots: usize,
    entries: Vec<WeatherHourly>,
    container: gtk::Box,
}

#[derive(Debug)]
pub enum WeatherHourlyStripInput {
    Update(Vec<WeatherHourly>),
    Clear,
}

#[relm4::component(pub)]
impl SimpleComponent for WeatherHourlyStrip {
    type Init = usize;
    type Input = WeatherHourlyStripInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "weather-hourly",
            set_homogeneous: true,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_visible(false);
        let model = WeatherHourlyStrip {
            hourly_slots: init,
            entries: Vec::new(),
            container: root.clone(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            WeatherHourlyStripInput::Update(entries) => {
                self.entries = entries;
                render_hourly_strip(
                    &self.container,
                    visible_hourly_slots(self.hourly_slots, self.entries.len()),
                    &self.entries,
                );
            }
            WeatherHourlyStripInput::Clear => {
                self.entries.clear();
                render_hourly_strip(&self.container, 0, &self.entries);
            }
        }
    }
}

pub fn visible_hourly_slots(configured: usize, available: usize) -> usize {
    configured.min(8).min(available)
}

fn render_hourly_strip(container: &gtk::Box, count: usize, entries: &[WeatherHourly]) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    container.set_visible(count > 0);
    for entry in entries.iter().take(count) {
        container.append(&build_hourly_col(entry));
    }
}

fn build_hourly_col(entry: &WeatherHourly) -> gtk::Box {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 4);
    col.add_css_class("weather-hourly-col");

    let time = gtk::Label::new(Some(&entry.time));
    time.add_css_class("weather-hourly-time");
    col.append(&time);

    let icon = gtk::Image::from_icon_name(&entry.icon);
    icon.set_pixel_size(24);
    col.append(&icon);

    let temp_label = gtk::Label::new(Some(&format!("{:.0}°", entry.temperature)));
    temp_label.add_css_class("weather-hourly-temp");
    col.append(&temp_label);

    col
}
