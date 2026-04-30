#![allow(unused_assignments)]

use chrono::{Local, NaiveDate};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::{animated_popover::AnimatedPopover, popover_shell::PopoverShell},
    services::{
        calendar_events::{CalendarDaySnapshot, MonthKey, State as CalendarState},
        clock::State as ClockState,
    },
};

use super::components::{
    calendar::{Calendar, CalendarInput, CalendarOutput},
    date::{Date, DateInput},
    events::{Events, EventsInput},
    world_clock::{WorldClock, WorldClockInput},
};

pub struct Popover {
    animation: AnimatedPopover,
    selected_date: NaiveDate,
    visible_month: MonthKey,
    clock: ClockState,
    calendar: CalendarState,
    date: Controller<Date>,
    calendar_view: Controller<Calendar>,
    world_clock: Controller<WorldClock>,
    events: Controller<Events>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub clock: ClockState,
    pub calendar: CalendarState,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateClock(ClockState),
    UpdateCalendar(CalendarState),
    CalendarOutput(CalendarOutput),
}

#[derive(Debug, Clone)]
pub enum PopoverOutput {
    VisibleMonthChanged(MonthKey),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "clock-popover",
            add_css_class: "popover-size-xlarge",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 0,
                        add_css_class: "clock-popover-layout",

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "clock-popover-left",

                            #[local_ref]
                            date_widget -> gtk::Box {},

                            #[local_ref]
                            calendar_widget -> gtk::Box {},

                            #[local_ref]
                            world_clock_widget -> gtk::Box {},
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "clock-popover-right",

                            #[local_ref]
                            events_widget -> gtk::Box {},
                        },
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let selected_date = Local::now().date_naive();
        let visible_month = MonthKey::from_date(selected_date);
        let date = Date::builder().launch(selected_date).detach();
        let calendar_view = Calendar::builder()
            .launch(selected_date)
            .forward(sender.input_sender(), PopoverInput::CalendarOutput);
        let world_clock = WorldClock::builder()
            .launch(init.clock.world.clone())
            .detach();
        let events = Events::builder().launch(selected_date).detach();

        let date_widget = date.widget().clone();
        let calendar_widget = calendar_view.widget().clone();
        let world_clock_widget = world_clock.widget().clone();
        let events_widget = events.widget().clone();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let mut model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            selected_date,
            visible_month,
            clock: init.clock,
            calendar: init.calendar,
            date,
            calendar_view,
            world_clock,
            events,
        };
        model.sync_all();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.animation.toggle(),
            PopoverInput::UpdateClock(clock) => {
                self.clock = clock;
                self.world_clock
                    .emit(WorldClockInput::Update(self.clock.world.clone()));
                self.events.emit(EventsInput::Tick);
            }
            PopoverInput::UpdateCalendar(calendar) => {
                self.calendar = calendar;
                self.sync_calendar_state();
            }
            PopoverInput::CalendarOutput(output) => match output {
                CalendarOutput::SelectedDate(date) => {
                    self.selected_date = date;
                    self.sync_selected_date();
                }
                CalendarOutput::VisibleMonthChanged(month) => {
                    self.visible_month = month;
                    self.sync_calendar_state();
                    let _ = sender.output(PopoverOutput::VisibleMonthChanged(month));
                }
            },
        }
    }
}

impl Popover {
    fn sync_all(&mut self) {
        self.sync_selected_date();
        self.sync_calendar_state();
        self.world_clock
            .emit(WorldClockInput::Update(self.clock.world.clone()));
    }

    fn sync_selected_date(&mut self) {
        self.date.emit(DateInput::SetDate(self.selected_date));
        self.calendar_view
            .emit(CalendarInput::SetDate(self.selected_date));
        self.sync_events();
    }

    fn sync_calendar_state(&mut self) {
        let month = self.calendar.month_cache.get(&self.visible_month).cloned();
        self.calendar_view.emit(CalendarInput::SetMonth(month));
        self.sync_events();
    }

    fn sync_events(&mut self) {
        self.events.emit(EventsInput::SetDate {
            date: self.selected_date,
            day: selected_day(&self.calendar, self.selected_date),
            loading: self
                .calendar
                .loading_months
                .contains(&MonthKey::from_date(self.selected_date)),
        });
    }
}

fn selected_day(state: &CalendarState, date: NaiveDate) -> Option<CalendarDaySnapshot> {
    let key = MonthKey::from_date(date);
    let calendar_date = crate::services::calendar_events::CalendarDate::from_naive_date(date);
    state
        .month_cache
        .get(&key)
        .and_then(|month| month.day_snapshots.get(&calendar_date))
        .cloned()
        .or_else(|| {
            state.month_cache.get(&key).map(|_| CalendarDaySnapshot {
                date: calendar_date,
                events: Vec::new(),
            })
        })
}
