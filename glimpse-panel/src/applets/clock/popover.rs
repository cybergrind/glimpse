use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use chrono::{Local, NaiveDate};
use glimpse::calendar::protocol::{CalendarDate, CalendarServiceCommand, CalendarServiceState};

use super::calendar::{Calendar, CalendarInit, Input as CalendarInput, Output as CalendarOutput};
use super::date::{Date, Input as DateInput};
use super::events::{Events, EventsInit, EventsInput, EventsOutput};
use super::world::WorldClock;
use crate::applets::clock::{
    applet::{month_key, resolve_selected_day_plan},
    config::TimezoneEntry,
    world::WorldClockInput,
};

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
    state: CalendarServiceState,
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
            .launch(CalendarInit { selected_date })
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
            state: CalendarServiceState::default(),
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
                    self.emit_selected_day(today, false);
                }
                if let Some(ref wc) = self.world_clock {
                    wc.emit(WorldClockInput::Tick);
                }
                self.events.emit(EventsInput::Tick);
            }
            PopoverInput::CalendarState(state) => {
                self.state = state;
                self.sync_from_state(&self.state);
            }
            PopoverInput::CalendarOutput(output) => {
                match output {
                    CalendarOutput::SelectedDate(date) => {
                        let month_changed = month_key(date) != month_key(self.selected_date);
                        self.selected_date = date;
                        self.follow_today = date == Local::now().date_naive();
                        self.date.emit(DateInput::SetDate(date));
                        self.emit_selected_day(date, month_changed);
                    }
                    CalendarOutput::LoadMonth { year, month } => {
                        let _ = sender.output(PopoverOutput::Command(
                            CalendarServiceCommand::LoadMonth { year, month },
                        ));
                    }
                }
            }
            PopoverInput::EventsOutput(output) => match output {
                EventsOutput::LoadDay { date } => {
                    let _ =
                        sender.output(PopoverOutput::Command(CalendarServiceCommand::LoadDay {
                            date: CalendarDate::from_naive_date(date),
                        }));
                }
            },
        }
    }
}

impl Popover {
    fn emit_selected_day(&self, date: NaiveDate, month_changed: bool) {
        let plan = resolve_selected_day_plan(&self.state, date, month_changed);
        self.events.emit(EventsInput::SetDate {
            date,
            day: plan.day,
            refresh: plan.refresh,
        });
    }

    fn sync_from_state(&self, state: &CalendarServiceState) {
        let month = state.month_cache.get(&month_key(self.selected_date)).cloned();
        let day_update = resolve_selected_day_plan(state, self.selected_date, false).day;

        match month {
            Some(month) => self.calendar.emit(CalendarInput::MonthData(month)),
            None => self.calendar.emit(CalendarInput::ClearMonth),
        }

        if let Some(day) = day_update {
            self.events.emit(EventsInput::Data(day));
        }
    }
}
