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
        let plan = resolve_selected_day_plan_with_hint(&self.state, date, month_changed);
        self.events.emit(EventsInput::SetDate {
            date,
            day: plan.day,
            refresh: plan.refresh,
        });
    }

    fn sync_from_state(&self, state: &CalendarServiceState) {
        let month_key = (self.selected_date.year(), self.selected_date.month());
        let month = state.month_cache.get(&month_key).cloned();
        let day_update = resolve_selected_day_plan(state, self.selected_date).day;

        match month {
            Some(month) => self.calendar.emit(CalendarInput::MonthData(month)),
            None => self.calendar.emit(CalendarInput::ClearMonth),
        }

        if let Some(day) = day_update {
            self.events.emit(EventsInput::Data(day));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectedDayPlan {
    day: Option<CalendarDaySnapshot>,
    refresh: bool,
}

fn resolve_selected_day_plan(
    state: &CalendarServiceState,
    selected_date: NaiveDate,
) -> SelectedDayPlan {
    resolve_selected_day_plan_with_hint(state, selected_date, false)
}

fn resolve_selected_day_plan_with_hint(
    state: &CalendarServiceState,
    selected_date: NaiveDate,
    month_changed: bool,
) -> SelectedDayPlan {
    if let Some(day) = state
        .day_cache
        .get(&CalendarDate::from_naive_date(selected_date))
        .cloned()
    {
        return SelectedDayPlan {
            day: Some(day),
            refresh: false,
        };
    }

    let month_key = (selected_date.year(), selected_date.month());
    if let Some(day) = state
        .month_cache
        .get(&month_key)
        .and_then(|month| {
            month
                .day_snapshots
                .get(&CalendarDate::from_naive_date(selected_date))
        })
        .cloned()
    {
        return SelectedDayPlan {
            day: Some(day),
            refresh: false,
        };
    }

    if let Some(day) = today_as_day_snapshot(state, selected_date) {
        return SelectedDayPlan {
            day: Some(day),
            refresh: false,
        };
    }

    if month_changed {
        return SelectedDayPlan {
            day: None,
            refresh: false,
        };
    }

    SelectedDayPlan {
        day: None,
        refresh: true,
    }
}

fn month_key(date: NaiveDate) -> (i32, u32) {
    (date.year(), date.month())
}

fn today_as_day_snapshot(
    state: &CalendarServiceState,
    selected_date: NaiveDate,
) -> Option<CalendarDaySnapshot> {
    let today = state.today.as_ref()?;
    if today.date.to_naive_date()? != selected_date {
        return None;
    }

    Some(CalendarDaySnapshot {
        date: today.date,
        events: today.events.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::calendar::protocol::{
        CalendarEvent, CalendarMonthSnapshot, CalendarServiceHealth, CalendarToday,
    };

    #[test]
    fn missing_selected_day_preserves_existing_list_until_loaded() {
        let selected_date = NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();
        let state = CalendarServiceState {
            health: CalendarServiceHealth::Ready,
            ..Default::default()
        };

        assert_eq!(
            resolve_selected_day_plan(&state, selected_date),
            SelectedDayPlan {
                day: None,
                refresh: true,
            }
        );
    }

    #[test]
    fn selected_day_uses_cached_snapshot_even_when_empty() {
        let selected_date = NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();
        let day = CalendarDaySnapshot {
            date: CalendarDate::from_naive_date(selected_date),
            events: Vec::new(),
        };
        let mut state = CalendarServiceState {
            health: CalendarServiceHealth::Ready,
            ..Default::default()
        };
        state.day_cache.insert(day.date, day.clone());

        assert_eq!(
            resolve_selected_day_plan(&state, selected_date),
            SelectedDayPlan {
                day: Some(day),
                refresh: false,
            }
        );
    }

    #[test]
    fn today_fallback_uses_today_snapshot_when_day_cache_is_missing() {
        let selected_date = NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();
        let day = CalendarDaySnapshot {
            date: CalendarDate::from_naive_date(selected_date),
            events: vec![CalendarEvent {
                title: "Meeting".into(),
                ..Default::default()
            }],
        };
        let state = CalendarServiceState {
            health: CalendarServiceHealth::Ready,
            today: Some(CalendarToday {
                date: day.date,
                events: day.events.clone(),
            }),
            ..Default::default()
        };

        assert_eq!(
            resolve_selected_day_plan(&state, selected_date),
            SelectedDayPlan {
                day: Some(day),
                refresh: false,
            }
        );
    }

    #[test]
    fn selected_day_uses_month_preloaded_day_snapshot() {
        let selected_date = NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();
        let day = CalendarDaySnapshot {
            date: CalendarDate::from_naive_date(selected_date),
            events: vec![CalendarEvent {
                title: "From month cache".into(),
                ..Default::default()
            }],
        };
        let mut month = CalendarMonthSnapshot {
            year: 2026,
            month: 4,
            ..Default::default()
        };
        month.day_snapshots.insert(day.date, day.clone());
        let mut state = CalendarServiceState {
            health: CalendarServiceHealth::Ready,
            ..Default::default()
        };
        state.month_cache.insert((2026, 4), month);

        assert_eq!(
            resolve_selected_day_plan(&state, selected_date),
            SelectedDayPlan {
                day: Some(day),
                refresh: false,
            }
        );
    }

    #[test]
    fn month_switch_without_local_data_skips_redundant_day_refresh() {
        let selected_date = NaiveDate::from_ymd_opt(2026, 5, 2).unwrap();
        let state = CalendarServiceState {
            health: CalendarServiceHealth::Ready,
            ..Default::default()
        };

        assert_eq!(
            resolve_selected_day_plan_with_hint(&state, selected_date, true),
            SelectedDayPlan {
                day: None,
                refresh: false,
            }
        );
    }
}
