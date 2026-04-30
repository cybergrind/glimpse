#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::key_value_grid::{KeyValueGrid, KeyValueGridInit, KeyValueGridInput, KeyValueItem},
    services::weather::model::{CurrentWeather, DailyForecast, State},
};

use super::super::format;

pub struct Details {
    rows: Vec<KeyValueItem>,
    grid: Controller<KeyValueGrid>,
}

#[derive(Debug)]
pub enum DetailsInput {
    Update(State),
}

#[relm4::component(pub)]
impl SimpleComponent for Details {
    type Init = ();
    type Input = DetailsInput;
    type Output = ();

    view! {
        gtk::Box {
            #[local_ref]
            grid_widget -> gtk::Box {},
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let grid = KeyValueGrid::builder()
            .launch(KeyValueGridInit { values: Vec::new() })
            .detach();
        let grid_widget = grid.widget().clone();
        let model = Details {
            rows: Vec::new(),
            grid,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            DetailsInput::Update(state) => {
                let rows = match state {
                    State::Ready(snapshot) => {
                        let today = snapshot
                            .forecast
                            .iter()
                            .find(|item| item.is_today)
                            .or_else(|| snapshot.forecast.first());
                        build_detail_rows(&snapshot.current, today)
                    }
                    State::Unknown | State::Loading | State::Unavailable(_) => Vec::new(),
                };
                if self.rows != rows {
                    self.grid.emit(KeyValueGridInput::Update(rows.clone()));
                    self.grid.widget().set_visible(!rows.is_empty());
                    self.rows = rows;
                }
            }
        }
    }
}

pub fn build_detail_rows(
    current: &CurrentWeather,
    today: Option<&DailyForecast>,
) -> Vec<KeyValueItem> {
    let high = today
        .map(|today| format::temperature(today.temperature_max))
        .unwrap_or_else(|| "—".into());
    let low = today
        .map(|today| format::temperature(today.temperature_min))
        .unwrap_or_else(|| "—".into());
    let sunrise = today
        .map(|today| display_time_or_dash(&today.sunrise))
        .unwrap_or_else(|| "—".into());
    let sunset = today
        .map(|today| display_time_or_dash(&today.sunset))
        .unwrap_or_else(|| "—".into());

    vec![
        item("High", high),
        item("Low", low),
        item("Humidity", format!("{}%", current.humidity)),
        item(
            "Wind",
            format!(
                "{} {:.0} km/h",
                current.wind_direction_label, current.wind_speed
            ),
        ),
        item("Pressure", format!("{:.0} hPa", current.pressure)),
        item("UV", format!("{:.0}", current.uv_index)),
        item("Sunrise", sunrise),
        item("Sunset", sunset),
    ]
}

fn item(label: &str, value: String) -> KeyValueItem {
    KeyValueItem {
        label: label.into(),
        value,
        visible: true,
    }
}

pub fn display_time_or_dash(value: &str) -> String {
    value
        .split('T')
        .nth(1)
        .filter(|value| !value.is_empty())
        .unwrap_or("—")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_detail_rows_returns_core_weather_items() {
        let current = CurrentWeather {
            humidity: 82,
            wind_speed: 18.0,
            wind_direction_label: "NW".into(),
            pressure: 1008.0,
            uv_index: 1.0,
            ..CurrentWeather::default()
        };
        let today = DailyForecast {
            temperature_min: 8.0,
            temperature_max: 14.0,
            sunrise: "2099-01-01T06:12".into(),
            sunset: "2099-01-01T19:48".into(),
            ..DailyForecast::default()
        };

        let rows = build_detail_rows(&current, Some(&today));

        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0].value, "14°");
        assert_eq!(rows[1].value, "8°");
        assert_eq!(rows[6].value, "06:12");
        assert_eq!(rows[7].value, "19:48");
    }
}
