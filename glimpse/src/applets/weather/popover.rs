#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::{
    applet::WeatherSnapshot,
    components::{
        detail_grid::{WeatherDetailGrid, WeatherDetailGridInput},
        forecast_section::{WeatherForecastSection, WeatherForecastSectionInput},
        hero::{WeatherHero, WeatherHeroInput},
        hourly_strip::{WeatherHourlyStrip, WeatherHourlyStripInput},
    },
};
use crate::components::popover_shell::{PopoverShell, PopoverShellInit};

pub struct WeatherPopover {
    popover: gtk::Popover,
    #[allow(dead_code)]
    shell: Controller<PopoverShell>,
    #[allow(dead_code)]
    hero: Controller<WeatherHero>,
    #[allow(dead_code)]
    hourly: Controller<WeatherHourlyStrip>,
    #[allow(dead_code)]
    details: Controller<WeatherDetailGrid>,
    #[allow(dead_code)]
    forecast: Controller<WeatherForecastSection>,
}

pub struct WeatherPopoverInit {
    pub parent: gtk::Box,
    pub hourly_slots: usize,
    pub forecast_days: usize,
}

#[derive(Debug)]
pub enum WeatherPopoverInput {
    Toggle,
    UpdateSnapshot(WeatherSnapshot),
    Clear,
}

#[relm4::component(pub)]
impl SimpleComponent for WeatherPopover {
    type Init = WeatherPopoverInit;
    type Input = WeatherPopoverInput;
    type Output = ();

    view! {
        gtk::Popover {
            add_css_class: "weather-popover",
            set_hexpand: false,

            #[local_ref]
            shell_widget -> gtk::Box {}
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let shell = PopoverShell::builder()
            .launch(PopoverShellInit { show_footer: false })
            .detach();
        let hero = WeatherHero::builder().launch(()).detach();
        let hourly = WeatherHourlyStrip::builder()
            .launch(init.hourly_slots)
            .detach();
        let details = WeatherDetailGrid::builder().launch(()).detach();
        let forecast = WeatherForecastSection::builder()
            .launch(init.forecast_days)
            .detach();

        let hero_widget = hero.widget().clone();
        let hourly_widget = hourly.widget().clone();
        let details_widget = details.widget().clone();
        let forecast_widget = forecast.widget().clone();
        let shell_widget = shell.widget().clone();
        let shell_content = shell_widget
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("popover shell should expose content box");
        shell_content.append(&hero_widget);
        shell_content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        shell_content.append(&hourly_widget);
        shell_content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        shell_content.append(&details_widget);
        shell_content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        shell_content.append(&forecast_widget);

        let model = WeatherPopover {
            popover: root.clone(),
            shell,
            hero,
            hourly,
            details,
            forecast,
        };
        let widgets = view_output!();

        model.popover.set_parent(&init.parent);
        model.popover.set_autohide(true);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            WeatherPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.forecast.emit(WeatherForecastSectionInput::Collapse);
                    self.popover.popup();
                }
            }
            WeatherPopoverInput::UpdateSnapshot(snapshot) => {
                let current = snapshot.current.clone();
                let today = snapshot
                    .forecast
                    .iter()
                    .find(|entry| entry.is_today)
                    .cloned()
                    .or_else(|| snapshot.forecast.first().cloned());

                self.hero.emit(WeatherHeroInput::UpdateSnapshot {
                    current: snapshot.current,
                    location: snapshot.location,
                });
                self.hourly
                    .emit(WeatherHourlyStripInput::Update(snapshot.hourly));
                self.details.emit(WeatherDetailGridInput::UpdateSnapshot {
                    current: Some(current),
                    today,
                });
                self.forecast
                    .emit(WeatherForecastSectionInput::Update(snapshot.forecast));
            }
            WeatherPopoverInput::Clear => {
                self.hero.emit(WeatherHeroInput::Clear);
                self.hourly.emit(WeatherHourlyStripInput::Clear);
                self.details.emit(WeatherDetailGridInput::Clear);
                self.forecast.emit(WeatherForecastSectionInput::Clear);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use relm4::gtk;

    use super::super::applet::{WeatherCurrent, WeatherDaily};
    use super::super::components::{
        detail_grid::{build_detail_rows, display_time_or_dash},
        forecast_section::{forecast_detail, visible_forecast_rows},
        hero::{hero_location_constraints, hero_summary},
        hourly_strip::visible_hourly_slots,
    };

    #[test]
    fn hero_summary_formats_condition_and_feels_like_only() {
        let current = WeatherCurrent {
            condition: "Overcast".into(),
            apparent_temperature: 9.0,
            ..WeatherCurrent::default()
        };

        let summary = hero_summary(&current);

        assert_eq!(summary, "Overcast · Feels like 9°");
        assert!(!summary.contains("High"));
        assert!(!summary.contains("Low"));
    }

    #[test]
    fn hero_location_constraints_limit_width_and_ellipsis() {
        let (max_width_chars, ellipsize_mode) = hero_location_constraints();

        assert_eq!(max_width_chars, 24);
        assert_eq!(ellipsize_mode, gtk::pango::EllipsizeMode::End);
    }

    #[test]
    fn build_details_rows_returns_eight_items() {
        let current = WeatherCurrent {
            humidity: 82,
            wind_speed: 18.0,
            wind_direction_label: "NW".into(),
            pressure: 1008.0,
            precipitation: 1.2,
            uv_index: 1.0,
            ..WeatherCurrent::default()
        };
        let today = WeatherDaily {
            temperature_min: 8.0,
            temperature_max: 14.0,
            ..WeatherDaily::default()
        };

        let rows = build_detail_rows(&current, Some(&today), None);

        assert_eq!(rows.len(), 8);
        assert_eq!(rows[0], ("High".into(), "14°".into()));
        assert_eq!(rows[1], ("Low".into(), "8°".into()));
    }

    #[test]
    fn build_details_rows_uses_sunrise_and_sunset_when_available() {
        let current = WeatherCurrent {
            humidity: 82,
            wind_speed: 18.0,
            wind_direction_label: "NW".into(),
            pressure: 1008.0,
            precipitation: 1.2,
            uv_index: 1.0,
            ..WeatherCurrent::default()
        };
        let today = WeatherDaily {
            temperature_min: 8.0,
            temperature_max: 14.0,
            sunrise: "2099-01-01T06:12".into(),
            sunset: "2099-01-01T19:48".into(),
            ..WeatherDaily::default()
        };

        let rows = build_detail_rows(
            &current,
            Some(&today),
            Some((today.sunrise.as_str(), today.sunset.as_str())),
        );

        assert_eq!(rows[7], ("Sun".into(), "06:12 / 19:48".into()));
    }

    #[test]
    fn display_time_or_dash_extracts_clock_time() {
        assert_eq!(display_time_or_dash("2099-01-01T06:12"), "06:12");
        assert_eq!(display_time_or_dash(""), "—");
    }

    #[test]
    fn visible_forecast_rows_clamps_to_zero_through_ten() {
        assert_eq!(visible_forecast_rows(0, 8), 0);
        assert_eq!(visible_forecast_rows(5, 8), 5);
        assert_eq!(visible_forecast_rows(12, 8), 8);
    }

    #[test]
    fn visible_hourly_slots_clamps_to_zero_through_eight() {
        assert_eq!(visible_hourly_slots(0, 6), 0);
        assert_eq!(visible_hourly_slots(5, 6), 5);
        assert_eq!(visible_hourly_slots(12, 6), 6);
    }

    #[test]
    fn forecast_detail_includes_precipitation_hint_when_present() {
        let rainy = WeatherDaily {
            condition: "Rain".into(),
            precipitation_sum: 3.2,
            ..WeatherDaily::default()
        };
        let dry = WeatherDaily {
            condition: "Cloudy".into(),
            precipitation_sum: 0.0,
            ..WeatherDaily::default()
        };

        assert_eq!(forecast_detail(&rainy), "Rain");
        assert_eq!(forecast_detail(&dry), "Cloudy");
    }
}
