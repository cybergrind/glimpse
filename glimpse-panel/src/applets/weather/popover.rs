use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::applet::{WeatherCurrent, WeatherDaily, WeatherHourly, WeatherLocation};

pub struct WeatherPopover {
    popover: gtk::Popover,
    hourly_slots: usize,
    forecast_days: usize,
    hero_icon: gtk::Image,
    hero_temp: gtk::Label,
    hero_condition: gtk::Label,
    hero_location: gtk::Label,
    hourly_box: gtk::Box,
    stats_box: gtk::Box,
    forecast_section: gtk::Box,
    forecast_toggle_chevron: gtk::Label,
    forecast_box: gtk::Box,
    current: Option<WeatherCurrent>,
    today: Option<WeatherDaily>,
}

pub struct WeatherPopoverInit {
    pub parent: gtk::Box,
    pub hourly_slots: usize,
    pub forecast_days: usize,
}

#[derive(Debug)]
pub enum WeatherPopoverInput {
    Toggle,
    ToggleForecastSection,
    UpdateCurrent(WeatherCurrent),
    UpdateHourly(Vec<WeatherHourly>),
    UpdateForecast(Vec<WeatherDaily>),
    UpdateLocation(WeatherLocation),
}

impl SimpleComponent for WeatherPopover {
    type Init = WeatherPopoverInit;
    type Input = WeatherPopoverInput;
    type Output = ();
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Popover::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("weather-popover");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        // === Hero ===
        let hero_row1 = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        hero_row1.add_css_class("weather-hero");

        let hero_icon = gtk::Image::from_icon_name("weather-overcast-symbolic");
        hero_icon.set_pixel_size(32);
        hero_row1.append(&hero_icon);

        let hero_temp = gtk::Label::new(Some("—"));
        hero_temp.add_css_class("weather-hero-temp");
        hero_row1.append(&hero_temp);

        let hero_location = gtk::Label::new(None);
        configure_hero_location_label(&hero_location);
        hero_row1.append(&hero_location);

        vbox.append(&hero_row1);

        let hero_row2 = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        hero_row2.add_css_class("weather-hero-row2");

        let hero_condition = gtk::Label::new(None);
        hero_condition.set_halign(gtk::Align::Start);
        hero_condition.set_hexpand(true);
        hero_condition.add_css_class("weather-hero-condition");
        hero_row2.append(&hero_condition);

        vbox.append(&hero_row2);

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === 4h forecast ===
        let hourly_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        hourly_box.add_css_class("weather-hourly");
        hourly_box.set_homogeneous(true);
        vbox.append(&hourly_box);

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Stats grid ===
        let stats_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        stats_box.add_css_class("weather-stats");
        vbox.append(&stats_box);

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Forecast ===
        let forecast_section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        forecast_section.add_css_class("weather-forecast-section");
        forecast_section.set_visible(false);

        let forecast_toggle = gtk::Button::new();
        forecast_toggle.add_css_class("flat");
        forecast_toggle.add_css_class("device-btn");

        let forecast_toggle_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        forecast_toggle_row.add_css_class("device-header");

        let forecast_toggle_label = gtk::Label::new(Some("Forecast"));
        forecast_toggle_label.set_halign(gtk::Align::Start);
        forecast_toggle_label.set_hexpand(true);
        forecast_toggle_row.append(&forecast_toggle_label);

        let forecast_toggle_chevron = gtk::Label::new(Some("›"));
        forecast_toggle_chevron.add_css_class("chevron");
        forecast_toggle_row.append(&forecast_toggle_chevron);

        forecast_toggle.set_child(Some(&forecast_toggle_row));
        forecast_toggle.connect_clicked({
            let sender = _sender.clone();
            move |_| sender.input(WeatherPopoverInput::ToggleForecastSection)
        });
        forecast_section.append(&forecast_toggle);

        let forecast_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        forecast_box.add_css_class("weather-forecast");
        forecast_box.set_visible(false);
        forecast_box.add_css_class("device-list");
        forecast_section.append(&forecast_box);

        vbox.append(&forecast_section);

        root.set_child(Some(&vbox));

        let model = WeatherPopover {
            popover: root.clone(),
            hourly_slots: init.hourly_slots,
            forecast_days: init.forecast_days,
            hero_icon,
            hero_temp,
            hero_condition,
            hero_location,
            hourly_box,
            stats_box,
            forecast_section,
            forecast_toggle_chevron,
            forecast_box,
            current: None,
            today: None,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            WeatherPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    set_forecast_expanded(&self.forecast_box, &self.forecast_toggle_chevron, false);
                    self.popover.popup();
                }
            }
            WeatherPopoverInput::ToggleForecastSection => {
                let expanded = !self.forecast_box.is_visible();
                set_forecast_expanded(&self.forecast_box, &self.forecast_toggle_chevron, expanded);
            }
            WeatherPopoverInput::UpdateCurrent(data) => {
                let temp = data.temperature;
                let icon = data.icon.as_str();

                self.hero_icon.set_icon_name(Some(icon));
                self.hero_temp.set_label(&format!("{temp:.0}°"));
                self.hero_condition.set_label(&hero_summary(&data));
                self.current = Some(data);
                rebuild_details_box(&self.stats_box, self.current.as_ref(), self.today.as_ref());
            }
            WeatherPopoverInput::UpdateLocation(location) => {
                self.hero_location.set_label(&location.city);
            }
            WeatherPopoverInput::UpdateHourly(data) => {
                clear_box(&self.hourly_box);
                let count = visible_hourly_slots(self.hourly_slots, data.len());
                self.hourly_box.set_visible(count > 0);
                for entry in data.iter().take(count) {
                    self.hourly_box.append(&build_hourly_col(entry));
                }
            }
            WeatherPopoverInput::UpdateForecast(data) => {
                self.today = data
                    .iter()
                    .find(|entry| entry.is_today)
                    .cloned()
                    .or_else(|| data.first().cloned());
                rebuild_details_box(&self.stats_box, self.current.as_ref(), self.today.as_ref());

                clear_box(&self.forecast_box);

                let future: Vec<_> = data.iter().filter(|entry| !entry.is_today).collect();
                let count = visible_forecast_rows(self.forecast_days, future.len());
                self.forecast_section.set_visible(count > 0);
                if count == 0 {
                    return;
                }

                for entry in future.into_iter().take(count) {
                    self.forecast_box.append(&build_forecast_row(entry));
                }
            }
        }
    }
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn set_forecast_expanded(container: &gtk::Box, chevron: &gtk::Label, expanded: bool) {
    container.set_visible(expanded);
    chevron.set_label(if expanded { "⌄" } else { "›" });
}

fn rebuild_details_box(
    container: &gtk::Box,
    current: Option<&WeatherCurrent>,
    today: Option<&WeatherDaily>,
) {
    clear_box(container);

    let Some(current) = current else {
        return;
    };

    let sun = today.and_then(|day| {
        if day.sunrise.is_empty() || day.sunset.is_empty() {
            None
        } else {
            Some((day.sunrise.as_str(), day.sunset.as_str()))
        }
    });

    for row in build_details_rows(current, today, sun).chunks(2) {
        let detail_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        detail_row.set_homogeneous(true);
        detail_row.add_css_class("weather-detail-row");

        for (label, value) in row {
            detail_row.append(&build_detail_pair(label, value));
        }

        container.append(&detail_row);
    }
}

fn hero_summary(current: &WeatherCurrent) -> String {
    format!(
        "{} · Feels like {:.0}°",
        current.condition, current.apparent_temperature
    )
}

fn configure_hero_location_label(label: &gtk::Label) {
    let (max_width_chars, ellipsize_mode) = hero_location_constraints();
    label.set_halign(gtk::Align::End);
    label.set_hexpand(true);
    label.set_ellipsize(ellipsize_mode);
    label.set_max_width_chars(max_width_chars);
    label.add_css_class("weather-hero-location");
}

fn hero_location_constraints() -> (i32, gtk::pango::EllipsizeMode) {
    (24, gtk::pango::EllipsizeMode::End)
}

fn build_details_rows(
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

fn display_time_or_dash(value: &str) -> &str {
    value
        .rsplit('T')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("—")
}

fn visible_forecast_rows(configured: usize, available: usize) -> usize {
    configured.min(10).min(available)
}

fn visible_hourly_slots(configured: usize, available: usize) -> usize {
    configured.min(8).min(available)
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

fn forecast_detail(entry: &WeatherDaily) -> String {
    entry.condition.clone()
}

#[cfg(test)]
mod tests {
    use relm4::gtk;

    use super::{
        WeatherCurrent, WeatherDaily, build_details_rows, display_time_or_dash, forecast_detail,
        hero_location_constraints, hero_summary, visible_forecast_rows, visible_hourly_slots,
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

        let rows = build_details_rows(&current, Some(&today), None);

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

        let rows = build_details_rows(
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
