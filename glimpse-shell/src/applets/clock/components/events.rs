#![allow(unused_assignments)]

use chrono::{Local, NaiveDate};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, prelude::*},
};

use crate::{
    applets::clock::format,
    components::section_header::SectionHeader,
    services::calendar_events::{CalendarDaySnapshot, CalendarEvent},
};

pub struct Events {
    selected_date: NaiveDate,
    events: Vec<CalendarEvent>,
    row_views: Vec<EventRowView>,
    empty_label: String,
    loading: bool,
    list_box: gtk::Box,
}

#[derive(Debug)]
pub enum EventsInput {
    SetDate {
        date: NaiveDate,
        day: Option<CalendarDaySnapshot>,
        loading: bool,
    },
    Tick,
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for EventRowView {
    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 2,
            add_css_class: "event-row",

            #[name = "title"]
            gtk::Label {
                add_css_class: "event-title",
                add_css_class: "action-row__title",
                set_xalign: 0.0,
                set_wrap: true,
            },

            #[name = "time"]
            gtk::Label {
                add_css_class: "event-time",
                add_css_class: "dim-label",
                add_css_class: "action-row__meta",
                set_xalign: 0.0,
            },
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Events {
    type Init = NaiveDate;
    type Input = EventsInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "events",

            #[template]
            SectionHeader {
                add_css_class: "events-header",

                #[template_child]
                title {
                    set_label: "Events",
                },
            },

            #[local_ref]
            list_box -> gtk::Box {
                add_css_class: "events-list",
                #[watch]
                set_visible: !model.events.is_empty(),
            },

            gtk::Label {
                add_css_class: "events-empty",
                add_css_class: "dim-label",
                add_css_class: "empty-state__subtitle",
                set_xalign: 0.0,
                #[watch]
                set_label: &model.empty_label,
                #[watch]
                set_visible: model.events.is_empty(),
            },
        }
    }

    fn init(
        date: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let mut model = Events {
            selected_date: date,
            events: Vec::new(),
            row_views: Vec::new(),
            empty_label: String::new(),
            loading: true,
            list_box: list_box.clone(),
        };
        model.sync_empty_label();
        model.sync_rows();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            EventsInput::SetDate { date, day, loading } => {
                self.selected_date = date;
                self.loading = loading && day.is_none();
                self.events = day.map(|day| day.events).unwrap_or_default();
                self.sync_empty_label();
                self.rebuild_rows();
            }
            EventsInput::Tick => self.sync_rows(),
        }
    }
}

impl Events {
    fn sync_empty_label(&mut self) {
        self.empty_label = if self.loading {
            "Loading...".into()
        } else if self.selected_date == Local::now().date_naive() {
            "No more events today".into()
        } else {
            "No events".into()
        };
    }

    fn sync_rows(&self) {
        let now = Local::now();
        for (row, event) in self.row_views.iter().zip(&self.events) {
            row.title.set_label(&event.title);
            row.time
                .set_label(&format::event_time(event, self.selected_date, now));
            row.time.set_visible(!row.time.label().is_empty());
        }
    }

    fn rebuild_rows(&mut self) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }
        self.row_views.clear();

        let now = Local::now();
        for event in &self.events {
            let row = EventRowView::init(());
            row.title.set_label(&event.title);
            row.time
                .set_label(&format::event_time(event, self.selected_date, now));
            row.time.set_visible(!row.time.label().is_empty());
            self.list_box.append(row.as_ref());
            self.row_views.push(row);
        }
    }
}
