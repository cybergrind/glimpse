use chrono::{Local, NaiveDate};
use glimpse::calendar::protocol::{CalendarDate, CalendarDaySnapshot};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use crate::applets::clock::event_row::{EventRow, EventRowInit, EventRowInput};

pub struct Events {
    selected_date: NaiveDate,
    rows: Vec<Controller<EventRow>>,
    list_box: gtk::Box,
    empty_label: gtk::Label,
    last_refresh_minute: Option<String>,
}

#[derive(Debug)]
pub enum EventsInput {
    Tick,
    SetDate(NaiveDate),
    Data(CalendarDaySnapshot),
    Clear,
}

#[derive(Debug)]
pub enum EventsOutput {
    LoadDay { date: NaiveDate },
}

pub struct EventsInit {
    pub selected_date: NaiveDate,
}

#[relm4::component(pub)]
impl Component for Events {
    type Init = EventsInit;
    type Input = EventsInput;
    type Output = EventsOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "events",

            gtk::Label {
                add_css_class: "section-title",
                add_css_class: "events-header",
                set_xalign: 0.0,
                set_label: "Events",
            },

            #[name = "list_box"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "events-list",
            },

            #[name = "empty_label"]
            gtk::Label {
                add_css_class: "events-empty",
                add_css_class: "dim-label",
                set_xalign: 0.0,
                set_visible: true,
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();

        let model = Events {
            selected_date: init.selected_date,
            rows: Vec::new(),
            list_box: widgets.list_box.clone(),
            empty_label: widgets.empty_label.clone(),
            last_refresh_minute: None,
        };
        model.update_empty_state();
        model.refresh_selected_day(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            EventsInput::Tick => {
                self.rows
                    .iter_mut()
                    .for_each(|row| row.emit(EventRowInput::Tick));
                self.refresh_today_if_needed(&sender);
            }
            EventsInput::SetDate(date) => {
                if self.selected_date != date {
                    self.selected_date = date;
                    self.last_refresh_minute = None;
                    self.update_empty_state();
                    self.refresh_selected_day(&sender);
                }
            }
            EventsInput::Data(day) => {
                self.replace_rows(day);
                self.last_refresh_minute = Some(current_minute_key());
            }
            EventsInput::Clear => {
                self.replace_rows(CalendarDaySnapshot {
                    date: CalendarDate::from_naive_date(self.selected_date),
                    events: Vec::new(),
                });
                self.last_refresh_minute = Some(current_minute_key());
            }
        }
    }
}

impl Events {
    fn refresh_selected_day(&self, sender: &ComponentSender<Self>) {
        let _ = sender.output(EventsOutput::LoadDay {
            date: self.selected_date,
        });
    }

    fn refresh_today_if_needed(&mut self, sender: &ComponentSender<Self>) {
        if self.selected_date != Local::now().date_naive() {
            return;
        }

        let current_key = current_minute_key();
        if self.last_refresh_minute.as_deref() == Some(current_key.as_str()) {
            return;
        }

        self.refresh_selected_day(sender);
    }

    fn replace_rows(&mut self, day: CalendarDaySnapshot) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }
        self.rows.clear();

        for event in day.events {
            let row = EventRow::builder()
                .launch(EventRowInit {
                    event,
                    selected_date: self.selected_date,
                })
                .detach();
            self.list_box.append(row.widget());
            self.rows.push(row);
        }

        self.update_empty_state();
        let has_rows = !self.rows.is_empty();
        self.list_box.set_visible(has_rows);
        self.empty_label.set_visible(!has_rows);
    }

    fn update_empty_state(&self) {
        if self.selected_date == Local::now().date_naive() {
            self.empty_label.set_label("No more events today");
        } else {
            self.empty_label.set_label("No events");
        }
    }
}

fn current_minute_key() -> String {
    Local::now().format("%Y-%m-%dT%H:%M").to_string()
}
