use chrono::{Local, Offset, Utc};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::applets::clock::TimezoneEntry;

pub struct TimezoneRow {
    name: String,
    timezone: String,
    format: String,
    time: String,
    offset: String,
    day_label: Option<&'static str>,
}

#[derive(Debug)]
pub enum TimezoneRowInput {
    Tick,
}

#[relm4::component(pub)]
impl SimpleComponent for TimezoneRow {
    type Init = TimezoneEntry;
    type Input = TimezoneRowInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            set_hexpand: false,
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
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = TimezoneRow {
            name: entry.name,
            timezone: entry.timezone,
            format: entry.format,
            time: String::new(),
            offset: String::new(),
            day_label: None,
        };
        model.update_time();
        sender.input_sender().emit(TimezoneRowInput::Tick);

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: TimezoneRowInput, _sender: ComponentSender<Self>) {
        match msg {
            TimezoneRowInput::Tick => self.update_time(),
        }
    }
}

impl TimezoneRow {
    fn update_time(&mut self) {
        let now_utc = Utc::now();
        let local_date = Local::now().date_naive();

        let tz: chrono_tz::Tz = self.timezone.parse().unwrap_or_else(|_| {
            tracing::warn!("invalid timezone: {}, falling back to UTC", self.timezone);
            chrono_tz::UTC
        });

        let dt = now_utc.with_timezone(&tz);

        let offset_secs = dt.offset().fix().local_minus_utc();
        let offset_h = offset_secs / 3600;
        let offset_m = (offset_secs.abs() % 3600) / 60;

        let day_diff = dt.date_naive().signed_duration_since(local_date).num_days();

        self.time = dt.format(self.format.as_str()).to_string();
        self.offset = if offset_m == 0 {
            format!("{offset_h:+}")
        } else {
            format!("{offset_h:+}:{offset_m:02}")
        };
        self.day_label = match day_diff {
            1 => Some("Tomorrow"),
            -1 => Some("Yesterday"),
            _ => None,
        };
    }
}
