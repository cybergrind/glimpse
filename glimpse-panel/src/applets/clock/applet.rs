use chrono::{Datelike, Local, NaiveDate};
use glimpse::calendar::{
    CalendarServiceHandle,
    protocol::{
        CalendarDate, CalendarDaySnapshot, CalendarServiceCommand, CalendarServiceState,
    },
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{
        self,
        prelude::{GestureSingleExt, OrientableExt, WidgetExt},
    },
};
use std::time::Duration;

use crate::applets::clock::{
    config::ClockConfig,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

pub struct Clock {
    pub value: String,
    pub config: ClockConfig,
    service: CalendarServiceHandle,
    popover: Controller<Popover>,
}

pub struct ClockInit {
    pub config: ClockConfig,
    pub service: CalendarServiceHandle,
}

#[derive(Debug, Clone)]
pub enum ClockInput {
    Tick,
    TogglePopover,
    CalendarState(CalendarServiceState),
    PopoverOutput(PopoverOutput),
    Unavailable,
}

#[derive(Debug, Clone)]
pub enum ClockCommandOutput {
    Tick,
    CalendarState(CalendarServiceState),
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Clock {
    type Init = ClockInit;
    type Input = ClockInput;
    type Output = ();
    type CommandOutput = ClockCommandOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "applet",
            add_css_class: "clock",
            add_css_class: "hoverable",
            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(ClockInput::TogglePopover);
                }
            },

            gtk::Label {
                #[watch]
                set_label: &model.value,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let today = Local::now().date_naive();
        let popover = Popover::builder()
            .launch(PopoverInit {
                parent: root.clone(),
                timezones: init.config.timezones.clone(),
            })
            .forward(sender.input_sender(), ClockInput::PopoverOutput);

        let model = Clock {
            popover,
            service: init.service.clone(),
            value: String::new(),
            config: init.config,
        };
        let widgets = view_output!();

        let service = init.service;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("clock applet: subscribing to calendar service");
                    let mut state_rx = service.subscribe();
                    let _ = out.send(ClockCommandOutput::CalendarState(state_rx.borrow().clone()));

                    loop {
                        if state_rx.changed().await.is_err() {
                            break;
                        }
                        let _ =
                            out.send(ClockCommandOutput::CalendarState(state_rx.borrow().clone()));
                    }

                    tracing::warn!("clock applet: calendar service state channel closed");
                    let _ = out.send(ClockCommandOutput::Unavailable);
                })
                .drop_on_shutdown()
        });

        let service = model.service.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    let today = CalendarDate::from_naive_date(today);
                    for command in initial_calendar_commands(today) {
                        if let Err(error) = service.send(command).await {
                            tracing::warn!(error = %error, "clock applet: failed to send initial calendar command");
                            break;
                        }
                    }
                })
                .drop_on_shutdown()
        });

        // Clock always uses a local timer — it only needs chrono::Local::now()
        // to format the time string. No daemon dependency needed.
        sender.command(|out, shutdown| {
            out.send(ClockCommandOutput::Tick).ok();
            shutdown
                .register(async move {
                    local_timer(out).await;
                })
                .drop_on_shutdown()
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match input {
            ClockInput::Tick => {
                self.value = Local::now().format(&self.config.format).to_string();
                if should_tick_popover(self.popover.widget().is_visible()) {
                    self.popover.emit(PopoverInput::Tick);
                }
            }
            ClockInput::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            ClockInput::CalendarState(state) => {
                self.popover.emit(PopoverInput::CalendarState(state));
            }
            ClockInput::PopoverOutput(output) => {
                self.handle_popover_output(output, sender);
            }
            ClockInput::Unavailable => {
                tracing::warn!("clock applet: calendar service unavailable");
                self.popover
                    .emit(PopoverInput::CalendarState(CalendarServiceState::default()));
            }
        }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            ClockCommandOutput::Tick => sender.input(ClockInput::Tick),
            ClockCommandOutput::CalendarState(state) => {
                sender.input(ClockInput::CalendarState(state))
            }
            ClockCommandOutput::Unavailable => sender.input(ClockInput::Unavailable),
        }
    }
}

impl Clock {
    fn handle_popover_output(&self, output: PopoverOutput, sender: ComponentSender<Self>) {
        match output {
            PopoverOutput::Command(command) => self.send_command(sender, command),
        }
    }

    fn send_command(&self, sender: ComponentSender<Self>, command: CalendarServiceCommand) {
        let service = self.service.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    if let Err(error) = service.send(command).await {
                        tracing::warn!(error = %error, "clock applet: failed to send calendar service command");
                    }
                })
                .drop_on_shutdown()
        });
    }
}

async fn local_timer(out: relm4::Sender<ClockCommandOutput>) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if out.send(ClockCommandOutput::Tick).is_err() {
            break;
        }
    }
}

fn initial_calendar_commands(today: CalendarDate) -> Vec<CalendarServiceCommand> {
    vec![CalendarServiceCommand::LoadMonth {
        year: today.year,
        month: today.month,
    }]
}

fn should_tick_popover(popover_visible: bool) -> bool {
    popover_visible
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedDayPlan {
    pub(crate) day: Option<CalendarDaySnapshot>,
    pub(crate) refresh: bool,
}

pub(crate) fn resolve_selected_day_plan(
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

    let month_key = month_key(selected_date);
    if let Some(day) = state
        .month_cache
        .get(&month_key)
        .and_then(|month| month.day_snapshots.get(&CalendarDate::from_naive_date(selected_date)))
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

pub(crate) fn month_key(date: NaiveDate) -> (i32, u32) {
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
    use std::collections::BTreeMap;

    #[test]
    fn initial_calendar_commands_only_load_visible_month() {
        let today = CalendarDate {
            year: 2026,
            month: 4,
            day: 11,
        };

        assert_eq!(
            initial_calendar_commands(today),
            vec![CalendarServiceCommand::LoadMonth {
                year: 2026,
                month: 4,
            }]
        );
    }

    #[test]
    fn hidden_popover_does_not_need_tick_forwarding() {
        assert!(!should_tick_popover(false));
        assert!(should_tick_popover(true));
    }

    #[test]
    fn missing_selected_day_requests_refresh_until_loaded() {
        let selected_date = NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();
        let state = CalendarServiceState {
            health: CalendarServiceHealth::Ready,
            ..Default::default()
        };

        let resolved = resolve_selected_day_plan(&state, selected_date, false);
        assert_eq!(resolved.day, None);
        assert!(resolved.refresh);
    }

    #[test]
    fn today_fallback_uses_today_snapshot_when_selected_date_matches_today() {
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

        let resolved = resolve_selected_day_plan(&state, selected_date, false);
        assert_eq!(resolved.day, Some(day));
        assert!(!resolved.refresh);
    }

    #[test]
    fn selected_day_resolution_prefers_day_cache_over_month_and_today() {
        let selected_date = NaiveDate::from_ymd_opt(2026, 4, 12).unwrap();
        let direct = CalendarDaySnapshot {
            date: CalendarDate::from_naive_date(selected_date),
            events: vec![CalendarEvent {
                title: "direct".into(),
                ..Default::default()
            }],
        };
        let month_day = CalendarDaySnapshot {
            date: CalendarDate::from_naive_date(selected_date),
            events: vec![CalendarEvent {
                title: "month".into(),
                ..Default::default()
            }],
        };

        let mut state = CalendarServiceState {
            health: CalendarServiceHealth::Ready,
            ..Default::default()
        };
        state.day_cache.insert(direct.date, direct.clone());
        state.month_cache.insert(
            (2026, 4),
            CalendarMonthSnapshot {
                year: 2026,
                month: 4,
                day_snapshots: BTreeMap::from([(month_day.date, month_day)]),
                ..Default::default()
            },
        );

        let resolved = resolve_selected_day_plan(&state, selected_date, false);
        assert_eq!(resolved.day, Some(direct));
        assert!(!resolved.refresh);
    }

    #[test]
    fn month_change_without_cached_day_clears_events_without_refresh() {
        let selected_date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let state = CalendarServiceState {
            health: CalendarServiceHealth::Ready,
            ..Default::default()
        };

        let resolved = resolve_selected_day_plan(&state, selected_date, true);
        assert_eq!(resolved.day, None);
        assert!(!resolved.refresh);
    }
}
