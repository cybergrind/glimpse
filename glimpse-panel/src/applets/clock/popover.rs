use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use std::sync::Arc;

use glimpse_client::Client;

use super::calendar::Calendar;
use super::date::Date;
use super::events::{Events, EventsInit, EventsInput};
use super::world::WorldClock;
use crate::applets::clock::{config::TimezoneEntry, world::WorldClockInput};

pub struct Popover {
    popover: gtk::Popover,
    #[allow(dead_code)]
    date: Controller<Date>,
    #[allow(dead_code)]
    calendar: Controller<Calendar>,
    #[allow(dead_code)]
    world_clock: Option<Controller<WorldClock>>,
    #[allow(dead_code)]
    events: Controller<Events>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub timezones: Vec<TimezoneEntry>,
    pub client: Option<Arc<Client>>,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    Tick,
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = ();

    view! {
        gtk::Popover {}
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        container.add_css_class("clock-popover");
        container.add_css_class("clock-popover-layout");

        let left = gtk::Box::new(gtk::Orientation::Vertical, 0);
        left.add_css_class("clock-popover-left");

        let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
        right.add_css_class("clock-popover-right");

        let date = Date::builder().launch(()).detach();
        left.append(date.widget());

        let calendar = Calendar::builder().launch(()).detach();
        left.append(calendar.widget());

        let world_clock = if init.timezones.is_empty() {
            None
        } else {
            let wc = WorldClock::builder().launch(init.timezones).detach();
            left.append(wc.widget());
            Some(wc)
        };

        let events = Events::builder()
            .launch(EventsInit {
                client: init.client,
            })
            .detach();
        right.append(events.widget());

        container.append(&left);
        container.append(&right);

        root.set_parent(&init.parent);
        root.set_child(Some(&container));

        let model = Popover {
            popover: root.clone(),
            date,
            calendar,
            world_clock,
            events,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            PopoverInput::Tick => {
                if let Some(ref wc) = self.world_clock {
                    wc.emit(WorldClockInput::Tick);
                }
                self.events.emit(EventsInput::Tick);
            }
        }
    }
}
