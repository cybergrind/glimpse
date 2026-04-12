#![allow(unused_assignments)]

use chrono::{Local, NaiveDate};
use glimpse::calendar::protocol::{CalendarDaySnapshot, CalendarEvent};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque},
    gtk::{self, prelude::*},
};

use crate::applets::clock::components::event_row::{EventRow, EventRowInit, EventRowInput};

pub struct Events {
    selected_date: NaiveDate,
    rows: FactoryVecDeque<EventRowItem>,
    show_list: bool,
    empty_label: String,
    last_refresh_minute: Option<String>,
    loading: bool,
}

struct EventRowItem {
    row: Controller<EventRow>,
}

impl FactoryComponent for EventRowItem {
    type Init = EventRowInit;
    type Input = EventRowInput;
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        let row = EventRow::builder().launch(init).detach();
        Self { row }
    }

    fn init_root(&self) -> Self::Root {
        self.row.widget().clone()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        _root: Self::Root,
        _returned_widget: &gtk::Widget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
    }

    fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        self.row.emit(msg);
    }
}

#[derive(Debug)]
pub enum EventsInput {
    Tick,
    SetDate {
        date: NaiveDate,
        day: Option<CalendarDaySnapshot>,
        refresh: bool,
    },
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

            #[local_ref]
            list_box -> gtk::Box {
                add_css_class: "events-list",
                #[watch]
                set_visible: model.show_list,
            },

            gtk::Label {
                add_css_class: "events-empty",
                add_css_class: "dim-label",
                set_xalign: 0.0,
                #[watch]
                set_label: &model.empty_label,
                #[watch]
                set_visible: !model.show_list,
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let rows = FactoryVecDeque::builder().launch(list_box.clone()).detach();

        let mut model = Events {
            selected_date: init.selected_date,
            rows,
            show_list: false,
            empty_label: String::new(),
            last_refresh_minute: None,
            loading: false,
        };
        model.sync_empty_label();

        let widgets = view_output!();
        model.refresh_selected_day(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            EventsInput::Tick => {
                let row_count = self.rows.guard().len();
                for index in 0..row_count {
                    self.rows.send(index, EventRowInput::Tick);
                }
                self.refresh_today_if_needed(&sender);
            }
            EventsInput::SetDate { date, day, refresh } => {
                let changed = self.selected_date != date;
                self.selected_date = date;
                self.last_refresh_minute = None;

                if let Some(day) = day {
                    self.set_day(day.events);
                    self.last_refresh_minute = Some(current_minute_key());
                } else if changed {
                    self.show_loading_state();
                } else {
                    self.sync_empty_label();
                }

                if refresh {
                    self.refresh_selected_day(&sender);
                }
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
        let current_key = current_minute_key();
        if !should_refresh_today(
            self.selected_date,
            Local::now().date_naive(),
            self.last_refresh_minute.as_deref(),
            &current_key,
        ) {
            return;
        }

        self.refresh_selected_day(sender);
    }

    fn set_day(&mut self, events: Vec<CalendarEvent>) {
        self.loading = false;

        let mut guard = self.rows.guard();
        guard.clear();
        let has_rows = !events.is_empty();
        for event in events {
            guard.push_back(EventRowInit {
                event,
                selected_date: self.selected_date,
            });
        }
        drop(guard);

        self.show_list = has_rows;
        self.sync_empty_label();
    }

    fn show_loading_state(&mut self) {
        self.rows.guard().clear();
        self.loading = true;
        self.show_list = false;
        self.sync_empty_label();
    }

    fn sync_empty_label(&mut self) {
        self.empty_label = empty_state_text(self.selected_date, self.loading);
    }
}

fn should_refresh_today(
    selected_date: NaiveDate,
    today: NaiveDate,
    last_refresh_minute: Option<&str>,
    current_minute: &str,
) -> bool {
    selected_date == today && last_refresh_minute != Some(current_minute)
}

fn empty_state_text(selected_date: NaiveDate, loading: bool) -> String {
    if loading {
        return "Loading...".into();
    }

    if selected_date == Local::now().date_naive() {
        "No more events today".into()
    } else {
        "No events".into()
    }
}

fn current_minute_key() -> String {
    Local::now().format("%Y-%m-%dT%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::calendar::protocol::{CalendarDate, CalendarDaySnapshot, CalendarEvent};
    use relm4::gtk;

    #[test]
    fn today_refresh_is_skipped_within_same_minute() {
        let key = "2026-04-12T10:15".to_string();
        assert!(!should_refresh_today(
            NaiveDate::from_ymd_opt(2026, 4, 12).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 12).unwrap(),
            Some(&key),
            &key,
        ));
    }

    #[test]
    fn event_list_becomes_visible_when_day_has_events() {
        if gtk::init().is_err() {
            return;
        }

        let selected_date = NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();
        let events = Events::builder()
            .launch(EventsInit { selected_date })
            .detach();

        events.emit(EventsInput::SetDate {
            date: selected_date,
            day: Some(CalendarDaySnapshot {
                date: CalendarDate::from_naive_date(selected_date),
                events: vec![CalendarEvent {
                    title: "Planning".into(),
                    ..Default::default()
                }],
            }),
            refresh: false,
        });

        let context = gtk::glib::MainContext::default();
        while context.pending() {
            context.iteration(false);
        }

        let root = events.widget();
        let list_box = root
            .first_child()
            .and_then(|header| header.next_sibling())
            .expect("events list should exist");

        assert!(list_box.is_visible());
    }
}
