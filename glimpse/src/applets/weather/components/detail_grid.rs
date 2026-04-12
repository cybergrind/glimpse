use relm4::{
    gtk::{self, prelude::*},
    ComponentParts, ComponentSender, SimpleComponent,
};

use super::super::applet::{WeatherCurrent, WeatherDaily};

pub struct WeatherDetailGrid {
    rows: Vec<(String, String)>,
    container: gtk::Box,
}

#[derive(Debug)]
pub enum WeatherDetailGridInput {
    UpdateSnapshot {
        current: Option<WeatherCurrent>,
        today: Option<WeatherDaily>,
    },
    Clear,
}

#[relm4::component(pub)]
impl SimpleComponent for WeatherDetailGrid {
    type Init = ();
    type Input = WeatherDetailGridInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "weather-stats",
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = WeatherDetailGrid {
            rows: Vec::new(),
            container: root.clone(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            WeatherDetailGridInput::UpdateSnapshot { current, today } => {
                self.rows = current
                    .as_ref()
                    .map(|current| {
                        let sun = today.as_ref().and_then(|day| {
                            if day.sunrise.is_empty() || day.sunset.is_empty() {
                                None
                            } else {
                                Some((day.sunrise.as_str(), day.sunset.as_str()))
                            }
                        });

                        build_detail_rows(current, today.as_ref(), sun)
                    })
                    .unwrap_or_default();
            }
            WeatherDetailGridInput::Clear => {
                self.rows.clear();
            }
        }
        self.container.set_visible(!self.rows.is_empty());
        render_detail_rows(&self.container, &self.rows);
    }
}

pub fn build_detail_rows(
    current: &WeatherCurrent,
    today: Option<&WeatherDaily>,
    sun: Option<(&str, &str)>,
) -> Vec<(String, String)> {
    let (high, low) = today
        .map(|day| {
            (
                format!("{:.0}°", day.temperature_max),
                format!("{:.0}°", day.temperature_min),
            )
        })
        .unwrap_or_else(|| ("—".into(), "—".into()));
    let (sunrise, sunset) = sun.unwrap_or(("—", "—"));
    let wind = if current.wind_direction_label.is_empty() {
        format!("{:.0} km/h", current.wind_speed)
    } else {
        format!(
            "{:.0} km/h {}",
            current.wind_speed, current.wind_direction_label
        )
    };

    vec![
        ("High".into(), high),
        ("Low".into(), low),
        ("Humidity".into(), format!("{}%", current.humidity)),
        ("Wind".into(), wind),
        ("Rain".into(), format!("{:.1} mm", current.precipitation)),
        ("Pressure".into(), format!("{:.0} hPa", current.pressure)),
        ("UV index".into(), format!("{:.0}", current.uv_index)),
        (
            "Sun".into(),
            format!(
                "{} / {}",
                display_time_or_dash(sunrise),
                display_time_or_dash(sunset)
            ),
        ),
    ]
}

pub fn display_time_or_dash(value: &str) -> &str {
    value
        .rsplit('T')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("—")
}

fn render_detail_rows(container: &gtk::Box, rows: &[(String, String)]) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    for row in rows.chunks(2) {
        let detail_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        detail_row.set_homogeneous(true);
        detail_row.add_css_class("weather-detail-row");

        for (label, value) in row {
            detail_row.append(&build_detail_pair(label, value));
        }

        container.append(&detail_row);
    }
}

fn build_detail_pair(label: &str, value: &str) -> gtk::Box {
    let pair = gtk::Box::new(gtk::Orientation::Vertical, 2);
    pair.add_css_class("weather-detail-pair");

    let key = gtk::Label::new(Some(label));
    key.set_halign(gtk::Align::Start);
    key.add_css_class("weather-detail-key");
    pair.append(&key);

    let val = gtk::Label::new(Some(value));
    val.set_halign(gtk::Align::Start);
    val.set_xalign(0.0);
    val.add_css_class("weather-detail-val");
    pair.append(&val);

    pair
}
