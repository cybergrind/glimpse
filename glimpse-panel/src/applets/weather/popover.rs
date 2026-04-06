use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::applet::{WeatherCurrent, WeatherDaily, WeatherHourly, WeatherLocation};

pub struct WeatherPopover {
    popover: gtk::Popover,
    hero_icon: gtk::Image,
    hero_temp: gtk::Label,
    hero_condition: gtk::Label,
    hero_location: gtk::Label,
    hourly_box: gtk::Box,
    stats_box: gtk::Box,
    forecast_box: gtk::Box,
}

pub struct WeatherPopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum WeatherPopoverInput {
    Toggle,
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

    fn init_root() -> Self::Root { gtk::Popover::new() }

    fn init(
        init: Self::Init, root: Self::Root, _sender: ComponentSender<Self>,
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
        hero_location.set_halign(gtk::Align::End);
        hero_location.set_hexpand(true);
        hero_location.add_css_class("weather-hero-location");
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
        let stats_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        stats_box.add_css_class("weather-stats");
        vbox.append(&stats_box);

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === 10-day forecast ===
        let forecast_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        forecast_box.add_css_class("weather-forecast");
        vbox.append(&forecast_box);

        root.set_child(Some(&vbox));

        let model = WeatherPopover {
            popover: root.clone(),
            hero_icon, hero_temp, hero_condition, hero_location,
            hourly_box, stats_box, forecast_box,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            WeatherPopoverInput::Toggle => {
                if self.popover.is_visible() { self.popover.popdown(); }
                else { self.popover.popup(); }
            }
            WeatherPopoverInput::UpdateCurrent(data) => {
                let temp = data.temperature;
                let icon = data.icon.as_str();
                let feels = data.apparent_temperature;
                let humidity = data.humidity;
                let wind = data.wind_speed;
                let wind_dir = data.wind_direction_label.as_str();
                let uv = data.uv_index;
                let pressure = data.pressure;
                let precip = data.precipitation;

                self.hero_icon.set_icon_name(Some(icon));
                self.hero_temp.set_label(&format!("{temp:.0}°"));
                self.hero_condition.set_label(&hero_summary(&data));

                clear_box(&self.stats_box);

                let row1 = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                row1.set_homogeneous(true);
                row1.append(&build_stat_tile("Feels like", &format!("{feels:.0}°")));
                row1.append(&build_stat_tile("Humidity", &format!("{humidity}%")));
                row1.append(&build_stat_tile("Wind", &format!("{wind:.0} km/h {wind_dir}")));
                self.stats_box.append(&row1);

                let row2 = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                row2.set_homogeneous(true);
                row2.append(&build_stat_tile("UV Index", &format!("{uv:.0}")));
                row2.append(&build_stat_tile("Pressure", &format!("{pressure:.0} hPa")));
                row2.append(&build_stat_tile("Precipitation", &format!("{precip:.1} mm")));
                self.stats_box.append(&row2);
            }
            WeatherPopoverInput::UpdateLocation(location) => {
                self.hero_location.set_label(&location.city);
            }
            WeatherPopoverInput::UpdateHourly(data) => {
                clear_box(&self.hourly_box);
                for entry in data.iter().take(5) {
                    self.hourly_box.append(&build_hourly_col(entry));
                }
            }
            WeatherPopoverInput::UpdateForecast(data) => {
                clear_box(&self.forecast_box);

                for entry in &data {
                    self.forecast_box.append(&build_forecast_row(entry));
                }
            }
        }
    }
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() { container.remove(&child); }
}

fn hero_summary(current: &WeatherCurrent) -> String {
    format!(
        "{} · Feels like {:.0}°",
        current.condition, current.apparent_temperature
    )
}

fn build_stat_tile(label: &str, value: &str) -> gtk::Box {
    let tile = gtk::Box::new(gtk::Orientation::Vertical, 2);
    tile.add_css_class("weather-stat-tile");

    let key = gtk::Label::new(Some(label));
    key.set_halign(gtk::Align::Start);
    key.add_css_class("weather-stat-key");
    tile.append(&key);

    let val = gtk::Label::new(Some(value));
    val.set_halign(gtk::Align::Start);
    val.add_css_class("weather-stat-val");
    tile.append(&val);

    tile
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

    let icon = gtk::Image::from_icon_name(&entry.icon);
    icon.set_pixel_size(16);
    row.append(&icon);

    let lo_label = gtk::Label::new(Some(&format!("{:.0}°", entry.temperature_min)));
    lo_label.set_width_chars(4);
    lo_label.set_halign(gtk::Align::End);
    lo_label.add_css_class("weather-forecast-lo");
    row.append(&lo_label);

    let hi_label = gtk::Label::new(Some(&format!("{:.0}°", entry.temperature_max)));
    hi_label.set_width_chars(4);
    hi_label.set_halign(gtk::Align::End);
    hi_label.add_css_class("weather-forecast-hi");
    row.append(&hi_label);

    let detail = if entry.precipitation_sum > 0.0 {
        format!("{} · {:.0}mm", entry.condition, entry.precipitation_sum)
    } else {
        entry.condition.clone()
    };
    let cond_label = gtk::Label::new(Some(&detail));
    cond_label.set_hexpand(true);
    cond_label.set_halign(gtk::Align::End);
    cond_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    cond_label.add_css_class("weather-forecast-cond");
    row.append(&cond_label);

    row
}

#[cfg(test)]
mod tests {
    use super::{hero_summary, WeatherCurrent};

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
}
