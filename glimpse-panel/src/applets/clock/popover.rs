use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::calendar::Calendar;
use super::date::Date;
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
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub timezones: Vec<TimezoneEntry>,
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
        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        container.add_css_class("clock-popover");

        let date = Date::builder().launch(()).detach();
        container.append(date.widget());

        let calendar = Calendar::builder().launch(()).detach();
        container.append(calendar.widget());

        let world_clock = if init.timezones.is_empty() {
            None
        } else {
            let wc = WorldClock::builder().launch(init.timezones).detach();
            container.append(wc.widget());
            Some(wc)
        };

        root.set_parent(&init.parent);
        root.set_child(Some(&container));

        let model = Popover {
            popover: root.clone(),
            date,
            calendar,
            world_clock,
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
            }
        }
    }
}
