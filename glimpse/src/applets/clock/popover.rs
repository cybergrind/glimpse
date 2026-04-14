#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use chrono::{Local, NaiveDate};
use glimpse::calendar::protocol::{CalendarDaySnapshot, CalendarMonthSnapshot};

use super::components::{
    calendar::{Calendar, CalendarInit, Input as CalendarInput, Output as CalendarOutput},
    date::{Date, Input as DateInput},
    events::{Events, EventsInit, EventsInput, EventsOutput},
    world::WorldClock,
};
use crate::applets::clock::{components::world::WorldClockInput, TimezoneEntry};
use crate::components::popover_shell::{PopoverShell, PopoverShellInit};

pub struct Popover {
    popover: gtk::Popover,
    #[allow(dead_code)]
    shell: Controller<PopoverShell>,
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
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    Tick,
    SetSelectedDate(NaiveDate),
    SetMonth(Option<CalendarMonthSnapshot>),
    SetSelectedDay {
        date: NaiveDate,
        day: Option<CalendarDaySnapshot>,
        refresh: bool,
    },
    CalendarOutput(CalendarOutput),
    EventsOutput(EventsOutput),
}

#[derive(Debug, Clone)]
pub enum PopoverOutput {
    SelectedDate(NaiveDate),
    LoadMonth { year: i32, month: u32 },
    LoadDay { date: NaiveDate },
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        gtk::Popover {
            add_css_class: "clock-popover",

            #[local_ref]
            shell_widget -> gtk::Box {}
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let selected_date = Local::now().date_naive();
        let shell = PopoverShell::builder()
            .launch(PopoverShellInit::default())
            .detach();
        let date = Date::builder().launch(()).detach();
        let calendar = Calendar::builder()
            .launch(CalendarInit { selected_date })
            .forward(sender.input_sender(), PopoverInput::CalendarOutput);

        let world_clock = if init.timezones.is_empty() {
            None
        } else {
            Some(WorldClock::builder().launch(init.timezones).detach())
        };

        let events = Events::builder()
            .launch(EventsInit { selected_date })
            .forward(sender.input_sender(), PopoverInput::EventsOutput);
        let date_widget = date.widget().clone();
        let calendar_widget = calendar.widget().clone();
        let world_widget = world_clock
            .as_ref()
            .map(|clock| clock.widget().clone())
            .unwrap_or_else(|| {
                let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
                widget.set_visible(false);
                widget
            });
        let events_widget = events.widget().clone();
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        layout.add_css_class("clock-popover-layout");
        let left = gtk::Box::new(gtk::Orientation::Vertical, 0);
        left.add_css_class("clock-popover-left");
        left.append(&date_widget);
        left.append(&calendar_widget);
        left.append(&world_widget);
        let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
        right.add_css_class("clock-popover-right");
        right.append(&events_widget);
        layout.append(&left);
        layout.append(&right);

        let shell_widget = shell.widget().clone();
        let shell_content = shell_widget
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("popover shell should expose content box");
        shell_content.append(&layout);

        let model = Popover {
            popover: root.clone(),
            shell,
            date,
            calendar,
            world_clock,
            events,
        };
        let widgets = view_output!();
        model.popover.set_parent(&init.parent);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
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
            PopoverInput::SetSelectedDate(date) => {
                self.date.emit(DateInput::SetDate(date));
                self.calendar.emit(CalendarInput::SetDate(date));
            }
            PopoverInput::SetMonth(Some(month)) => {
                self.calendar.emit(CalendarInput::MonthData(month));
            }
            PopoverInput::SetMonth(None) => {
                self.calendar.emit(CalendarInput::ClearMonth);
            }
            PopoverInput::SetSelectedDay { date, day, refresh } => {
                self.events
                    .emit(EventsInput::SetDate { date, day, refresh });
            }
            PopoverInput::CalendarOutput(output) => match output {
                CalendarOutput::SelectedDate(date) => {
                    let _ = sender.output(PopoverOutput::SelectedDate(date));
                }
                CalendarOutput::LoadMonth { year, month } => {
                    let _ = sender.output(PopoverOutput::LoadMonth { year, month });
                }
            },
            PopoverInput::EventsOutput(output) => match output {
                EventsOutput::LoadDay { date } => {
                    let _ = sender.output(PopoverOutput::LoadDay { date });
                }
            },
        }
    }
}
