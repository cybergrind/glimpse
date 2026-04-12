#![allow(unused_assignments)]

use relm4::{
    gtk::{self, prelude::*},
    ComponentParts, ComponentSender, SimpleComponent,
};

use super::super::applet::WeatherDaily;

pub struct WeatherForecastSection {
    forecast_days: usize,
    expanded: bool,
    rows: Vec<WeatherDaily>,
    container: gtk::Box,
    rows_box: gtk::Box,
    chevron: gtk::Label,
}

#[derive(Debug)]
pub enum WeatherForecastSectionInput {
    Update(Vec<WeatherDaily>),
    ToggleExpanded,
    Collapse,
    Clear,
}

#[relm4::component(pub)]
impl SimpleComponent for WeatherForecastSection {
    type Init = usize;
    type Input = WeatherForecastSectionInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "weather-forecast-section",

            gtk::Button {
                add_css_class: "flat",
                add_css_class: "device-btn",
                connect_clicked => WeatherForecastSectionInput::ToggleExpanded,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    add_css_class: "device-header",

                    gtk::Label {
                        set_label: "Forecast",
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,
                    },

                    #[name(chevron)]
                    gtk::Label {
                        set_label: "›",
                        add_css_class: "chevron",
                    },
                },
            },

            #[name(rows_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "weather-forecast",
                add_css_class: "device-list",
                set_visible: false,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        let model = WeatherForecastSection {
            forecast_days: init,
            expanded: false,
            rows: Vec::new(),
            container: root.clone(),
            rows_box: widgets.rows_box.clone(),
            chevron: widgets.chevron.clone(),
        };
        model.container.set_visible(false);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            WeatherForecastSectionInput::Update(entries) => {
                let available = entries.iter().filter(|entry| !entry.is_today).count();
                let future = entries
                    .into_iter()
                    .filter(|entry| !entry.is_today)
                    .take(visible_forecast_rows(self.forecast_days, available))
                    .collect::<Vec<_>>();
                self.rows = future;
            }
            WeatherForecastSectionInput::ToggleExpanded => {
                if !self.rows.is_empty() {
                    self.expanded = !self.expanded;
                }
            }
            WeatherForecastSectionInput::Collapse => {
                self.expanded = false;
            }
            WeatherForecastSectionInput::Clear => {
                self.expanded = false;
                self.rows.clear();
            }
        }
        self.container.set_visible(!self.rows.is_empty());
        self.rows_box
            .set_visible(self.expanded && !self.rows.is_empty());
        self.chevron
            .set_label(if self.expanded { "⌄" } else { "›" });
        render_forecast_rows(&self.rows_box, &self.rows);
    }
}

pub fn visible_forecast_rows(configured: usize, available: usize) -> usize {
    configured.min(10).min(available)
}

pub fn forecast_detail(entry: &WeatherDaily) -> String {
    entry.condition.clone()
}

fn render_forecast_rows(container: &gtk::Box, rows: &[WeatherDaily]) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    for entry in rows {
        container.append(&build_forecast_row(entry));
    }
}

fn build_forecast_row(entry: &WeatherDaily) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("weather-forecast-row");

    if entry.is_today {
        row.add_css_class("weather-forecast-today");
    }

    let day = gtk::Label::new(Some(&entry.day_name));
    day.set_width_chars(5);
    day.set_halign(gtk::Align::Start);
    day.add_css_class("weather-forecast-day");
    row.append(&day);

    let temps = gtk::Label::new(Some(&format!(
        "{:.0}° / {:.0}°",
        entry.temperature_min, entry.temperature_max
    )));
    temps.set_width_chars(10);
    temps.set_halign(gtk::Align::Start);
    temps.set_xalign(0.0);
    temps.add_css_class("weather-forecast-temps");
    row.append(&temps);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    row.append(&spacer);

    let detail = forecast_detail(entry);
    let cond_label = gtk::Label::new(Some(&detail));
    cond_label.set_halign(gtk::Align::End);
    cond_label.set_xalign(1.0);
    cond_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    cond_label.add_css_class("weather-forecast-cond");
    row.append(&cond_label);

    let icon = gtk::Image::from_icon_name(&entry.icon);
    icon.set_pixel_size(16);
    row.append(&icon);

    row
}
