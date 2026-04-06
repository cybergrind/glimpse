use std::sync::Arc;

use glimpse_client::Client;
use glimpse_types::CalendarToday;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use crate::applets::clock::event_row::{EventRow, EventRowInput};

pub struct Events {
    rows: Vec<Controller<EventRow>>,
    list_box: gtk::Box,
    empty_label: gtk::Label,
}

#[derive(Debug)]
pub enum EventsInput {
    Tick,
    Data(CalendarToday),
    Clear,
}

pub struct EventsInit {
    pub client: Option<Arc<Client>>,
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
                set_label: "No more events today",
                set_visible: true,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();

        let model = Events {
            rows: Vec::new(),
            list_box: widgets.list_box.clone(),
            empty_label: widgets.empty_label.clone(),
        };

        if let Some(client) = init.client {
            sender.command(move |out, shutdown| {
                shutdown
                    .register(async move {
                        tracing::info!("clock events: subscribing");
                        let mut sub = match client.subscribe("calendar.today").await {
                            Ok(sub) => sub,
                            Err(e) => {
                                tracing::warn!("clock events: subscribe failed: {e}");
                                let _ = out.send(EventsInput::Clear);
                                return;
                            }
                        };

                        while let Some(event) = sub.next().await {
                            match serde_json::from_value::<CalendarToday>(event.data) {
                                Ok(today) => {
                                    let _ = out.send(EventsInput::Data(today));
                                }
                                Err(e) => {
                                    tracing::warn!("clock events: invalid payload: {e}");
                                }
                            }
                        }

                        let _ = out.send(EventsInput::Clear);
                    })
                    .drop_on_shutdown()
            });
        }

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

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            EventsInput::Tick => {
                self.rows
                    .iter_mut()
                    .for_each(|row| row.emit(EventRowInput::Tick));
            }
            EventsInput::Data(today) => {
                self.replace_rows(today);
            }
            EventsInput::Clear => {
                self.replace_rows(CalendarToday {
                    date: String::new(),
                    events: Vec::new(),
                });
            }
        }
    }
}

impl Events {
    fn replace_rows(&mut self, today: CalendarToday) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }
        self.rows.clear();

        for event in today.events {
            let row = EventRow::builder().launch(event).detach();
            self.list_box.append(row.widget());
            self.rows.push(row);
        }

        let has_rows = !self.rows.is_empty();
        self.list_box.set_visible(has_rows);
        self.empty_label.set_visible(!has_rows);
    }
}
