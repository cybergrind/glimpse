use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use chrono::{Datelike, Local, NaiveDate};
use glimpse::calendar::protocol::{
    CalendarDate, CalendarDaySnapshot, CalendarServiceCommand, CalendarServiceState,
};

use super::calendar::{Calendar, CalendarInit, Input as CalendarInput, Output as CalendarOutput};
use super::date::{Date, Input as DateInput};
use super::events::{Events, EventsInit, EventsInput, EventsOutput};
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
    selected_date: NaiveDate,
    follow_today: bool,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub timezones: Vec<TimezoneEntry>,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    Tick,
    CalendarState(CalendarServiceState),
    CalendarOutput(CalendarOutput),
    EventsOutput(EventsOutput),
}

#[derive(Debug, Clone)]
pub enum PopoverOutput {
    Command(CalendarServiceCommand),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        gtk::Popover {}
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let selected_date = Local::now().date_naive();
        root.add_css_class("clock-popover");

        let container = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        container.add_css_class("clock-popover-layout");

        let left = gtk::Box::new(gtk::Orientation::Vertical, 0);
        left.add_css_class("clock-popover-left");

        let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
        right.add_css_class("clock-popover-right");

        let date = Date::builder().launch(()).detach();
        left.append(date.widget());

        let calendar = Calendar::builder()
            .launch(CalendarInit {
                selected_date,
            })
            .forward(sender.input_sender(), PopoverInput::CalendarOutput);
        left.append(calendar.widget());

        let world_clock = if init.timezones.is_empty() {
            None
        } else {
            let wc = WorldClock::builder().launch(init.timezones).detach();
            left.append(wc.widget());
            Some(wc)
        };

        let events = Events::builder()
            .launch(EventsInit { selected_date })
            .forward(sender.input_sender(), PopoverInput::EventsOutput);
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
            selected_date,
            follow_today: true,
        };
        let widgets = view_output!();
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
                let today = Local::now().date_naive();
                if self.follow_today && self.selected_date != today {
                    self.selected_date = today;
                    self.date.emit(DateInput::SetDate(today));
                    self.calendar.emit(CalendarInput::SetDate(today));
                    self.events.emit(EventsInput::SetDate(today));
                }
                if let Some(ref wc) = self.world_clock {
                    wc.emit(WorldClockInput::Tick);
                }
                self.events.emit(EventsInput::Tick);
            }
            PopoverInput::CalendarState(state) => {
                self.sync_from_state(state);
            }
            PopoverInput::CalendarOutput(output) => match output {
                CalendarOutput::SelectedDate(date) => {
                    self.selected_date = date;
                    self.follow_today = date == Local::now().date_naive();
                    self.date.emit(DateInput::SetDate(date));
                    self.events.emit(EventsInput::SetDate(date));
                }
                CalendarOutput::LoadMonth { year, month } => {
                    let _ = sender.output(PopoverOutput::Command(
                        CalendarServiceCommand::LoadMonth { year, month },
                    ));
                }
            },
            PopoverInput::EventsOutput(output) => match output {
                EventsOutput::LoadDay { date } => {
                    let _ = sender.output(PopoverOutput::Command(
                        CalendarServiceCommand::LoadDay {
                            date: CalendarDate::from_naive_date(date),
                        },
                    ));
                }
            },
        }
    }
}

impl Popover {
    fn sync_from_state(&self, state: CalendarServiceState) {
        let month_key = (self.selected_date.year(), self.selected_date.month());
        let month = state.month_cache.get(&month_key).cloned();
        let day = state
            .day_cache
            .get(&CalendarDate::from_naive_date(self.selected_date))
            .cloned()
            .or_else(|| today_as_day_snapshot(&state, self.selected_date));

        match month {
            Some(month) => self.calendar.emit(CalendarInput::MonthData(month)),
            None => self.calendar.emit(CalendarInput::ClearMonth),
        }

        match day {
            Some(day) => self.events.emit(EventsInput::Data(day)),
            None => self.events.emit(EventsInput::Clear),
        }
    }
}

fn today_as_day_snapshot(state: &CalendarServiceState, selected_date: NaiveDate) -> Option<CalendarDaySnapshot> {
    let today = state.today.as_ref()?;
    if today.date.to_naive_date()? != selected_date {
        return None;
    }

    Some(CalendarDaySnapshot {
        date: today.date,
        events: today.events.clone(),
    })
}
