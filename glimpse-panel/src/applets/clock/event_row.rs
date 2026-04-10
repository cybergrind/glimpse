use chrono::{DateTime, Local, NaiveDate};
use glimpse::calendar::protocol::CalendarEvent;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone)]
pub struct EventRowInit {
    pub event: CalendarEvent,
    pub selected_date: NaiveDate,
}

pub struct EventRow {
    event: CalendarEvent,
    selected_date: NaiveDate,
    timing_label: String,
}

#[derive(Debug)]
pub enum EventRowInput {
    Tick,
}

#[relm4::component(pub)]
impl SimpleComponent for EventRow {
    type Init = EventRowInit;
    type Input = EventRowInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 2,
            add_css_class: "event-row",

            gtk::Label {
                add_css_class: "event-title",
                set_xalign: 0.0,
                set_wrap: true,
                #[watch]
                set_label: &model.event.title,
            },

            gtk::Label {
                add_css_class: "event-time",
                add_css_class: "dim-label",
                set_xalign: 0.0,
                #[watch]
                set_label: &model.timing_label,
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = EventRow {
            event: init.event,
            selected_date: init.selected_date,
            timing_label: String::new(),
        };
        model.refresh_label();

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: EventRowInput, _sender: ComponentSender<Self>) {
        match msg {
            EventRowInput::Tick => self.refresh_label(),
        }
    }
}

impl EventRow {
    fn refresh_label(&mut self) {
        let Some((start, end)) = event_times(&self.event) else {
            self.timing_label.clear();
            return;
        };
        self.timing_label = format_timing_line(start, end, self.selected_date, Local::now());
    }
}

fn event_times(event: &CalendarEvent) -> Option<(DateTime<Local>, DateTime<Local>)> {
    let start = DateTime::parse_from_rfc3339(&event.start).ok()?;
    let end = DateTime::parse_from_rfc3339(&event.end).ok()?;
    Some((start.with_timezone(&Local), end.with_timezone(&Local)))
}

pub fn format_timing_line(
    start: DateTime<Local>,
    end: DateTime<Local>,
    selected_date: NaiveDate,
    now: DateTime<Local>,
) -> String {
    if is_all_day_event(start, end) {
        return "All day".into();
    }

    if selected_date != now.date_naive() {
        return format!(
            "{} · {}",
            start.format("%H:%M"),
            format_duration(start, end)
        );
    }

    if now >= start && now < end {
        return format!("now · ends {}", end.format("%H:%M"));
    }

    let duration = format_duration(start, end);
    let until_start = start - now;

    if until_start.num_minutes() < 60 {
        format!("in {} min · {}", until_start.num_minutes(), duration)
    } else {
        format!("{} · {}", start.format("%H:%M"), duration)
    }
}

fn format_duration(start: DateTime<Local>, end: DateTime<Local>) -> String {
    format!("{} min", (end - start).num_minutes())
}

fn is_all_day_event(start: DateTime<Local>, end: DateTime<Local>) -> bool {
    start.time() == chrono::NaiveTime::MIN
        && end.time() == chrono::NaiveTime::MIN
        && (end - start).num_days() >= 1
}
