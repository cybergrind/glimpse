use chrono::{Local, Offset, Utc};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::applets::clock::config::TimezoneEntry;

pub struct TimezoneRow {
    name: String,
    time: String,
    offset: String,
    day_label: Option<&'static str>,
}

#[derive(Debug)]
pub enum Input {}

#[relm4::component(pub)]
impl SimpleComponent for TimezoneRow {
    type Init = TimezoneEntry;
    type Input = Input;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            add_css_class: "world-clock-timezone",

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_hexpand: true,
                set_spacing: 6,
                set_valign: gtk::Align::Center,

                gtk::Label {
                    add_css_class: "world-clock-city",
                    set_xalign: 0.0,
                    #[watch]
                    set_label: &model.name,
                },

                gtk::Label {
                    add_css_class: "caption",
                    add_css_class: "dim-label",
                    #[watch]
                    set_label: model.day_label.unwrap_or(""),
                    #[watch]
                    set_visible: model.day_label.is_some(),
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 6,
                set_valign: gtk::Align::Center,
                set_halign: gtk::Align::End,

                gtk::Label {
                    add_css_class: "world-clock-time",
                    #[watch]
                    set_label: &model.time,
                },

                gtk::Label {
                    add_css_class: "world-clock-tz",
                    add_css_class: "dim-label",
                    #[watch]
                    set_label: &model.offset,
                },
            },
        }
    }

    fn init(
        entry: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let now_utc = Utc::now();
        let local_date = Local::now().date_naive();

        let tz: chrono_tz::Tz = entry.timezone.parse().unwrap_or_else(|_| {
            tracing::warn!("invalid timezone: {}, falling back to UTC", entry.timezone);
            chrono_tz::UTC
        });

        let dt = now_utc.with_timezone(&tz);

        let offset_secs = dt.offset().fix().local_minus_utc();
        let offset_h = offset_secs / 3600;
        let offset_m = (offset_secs.abs() % 3600) / 60;

        let tz_date = dt.date_naive();
        let day_diff = tz_date.signed_duration_since(local_date).num_days();

        let model = TimezoneRow {
            name: entry.name,
            time: dt.format(entry.format.as_str()).to_string(),
            offset: if offset_m == 0 {
                format!("{offset_h:+}")
            } else {
                format!("{offset_h:+}:{offset_m:02}")
            },
            day_label: match day_diff {
                1 => Some("Tomorrow"),
                -1 => Some("Yesterday"),
                _ => None,
            },
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Input, _sender: ComponentSender<Self>) {
        match msg {}
    }
}
