use std::sync::Arc;

use chrono::{Local, NaiveDate};
use glimpse_client::Client;
use glimpse_types::CalendarDay;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use crate::applets::clock::event_row::{EventRow, EventRowInit, EventRowInput};

pub struct Events {
    client: Option<Arc<Client>>,
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
    Data(CalendarDay),
    Clear,
}

pub struct EventsInit {
    pub client: Option<Arc<Client>>,
    pub selected_date: NaiveDate,
}

#[relm4::component(pub)]
impl Component for Events {
    type Init = EventsInit;
    type Input = EventsInput;
    type Output = ();
    type CommandOutput = EventsInput;

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
            client: init.client,
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

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(msg, sender, root);
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
                self.replace_rows(CalendarDay {
                    date: self.selected_date.format("%F").to_string(),
                    events: Vec::new(),
                });
                self.last_refresh_minute = Some(current_minute_key());
            }
        }
    }
}

impl Events {
    fn refresh_selected_day(&self, sender: &ComponentSender<Self>) {
        let Some(client) = self.client.clone() else {
            sender.input(EventsInput::Clear);
            return;
        };
        let date = self.selected_date.format("%F").to_string();
        sender.command(move |out, _shutdown| async move {
            let result = client
                .call("calendar.day", serde_json::json!({ "date": date }))
                .await
                .and_then(|value| serde_json::from_value::<CalendarDay>(value).map_err(Into::into));

            match result {
                Ok(day) => {
                    let _ = out.send(EventsInput::Data(day));
                }
                Err(e) => {
                    tracing::warn!("clock events: failed to fetch day: {e}");
                    let _ = out.send(EventsInput::Clear);
                }
            }
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

    fn replace_rows(&mut self, day: CalendarDay) {
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
