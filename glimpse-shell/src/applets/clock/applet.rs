use chrono::Local;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        calendar_events::{self, MonthKey, State as CalendarState},
        clock::{self, State as ClockState, TimezoneConfig},
        framework::ServiceCommand,
    },
};

use super::{
    format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(alias = "format")]
    pub label_format: String,
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
    pub timezones: Vec<TimezoneConfig>,
    pub tick_interval: u64,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid clock applet config, using defaults");
                Self::default()
            }
        }
    }

    fn clock_config(&self) -> clock::Config {
        clock::Config {
            timezones: self.timezones.clone(),
            tick_interval: self.tick_interval,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            label_format: format::DEFAULT_LABEL_FORMAT.into(),
            tooltip_format: format::DEFAULT_TOOLTIP_FORMAT.into(),
            timezones: Vec::new(),
            tick_interval: clock::Config::default().tick_interval,
        }
    }
}

pub struct Applet {
    config: Config,
    label: String,
    tooltip: String,
    clock: clock::ClockHandle,
    calendar: calendar_events::CalendarEventsHandle,
    clock_state: ClockState,
    calendar_state: CalendarState,
    popover: Controller<Popover>,
    clock_cancel: CancellationToken,
    calendar_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub clock: clock::ClockHandle,
    pub calendar: calendar_events::CalendarEventsHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ClockStateChanged(ClockState),
    CalendarStateChanged(CalendarState),
    Reconfigure(Config),
    TogglePopover,
    PopoverOutput(PopoverOutput),
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::TogglePopover);
                },
            },

            gtk::Label {
                set_valign: gtk::Align::Center,
                #[watch]
                set_label: &model.label,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let clock_state = init.clock.snapshot();
        let calendar_state = init.calendar.snapshot();
        let popover = Popover::builder()
            .launch(PopoverInit {
                parent: root.clone(),
                clock: clock_state.clone(),
                calendar: calendar_state.clone(),
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let model = Applet {
            label: format::label(&init.config.label_format, &clock_state),
            tooltip: format::tooltip(&init.config.tooltip_format, &clock_state),
            config: init.config,
            clock: init.clock,
            calendar: init.calendar,
            clock_state,
            calendar_state,
            popover,
            clock_cancel: CancellationToken::new(),
            calendar_cancel: CancellationToken::new(),
        };

        subscribe_clock(&model.clock, model.clock_cancel.clone(), &sender);
        subscribe_calendar(&model.calendar, model.calendar_cancel.clone(), &sender);
        model.configure_clock();
        model.preload_month(MonthKey::from_date(Local::now().date_naive()));

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ClockStateChanged(state) => self.apply_clock_state(state),
            Input::CalendarStateChanged(state) => self.apply_calendar_state(state),
            Input::Reconfigure(config) => {
                self.config = config;
                self.configure_clock();
                self.apply_clock_state(self.clock.snapshot());
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
                self.sync_popover();
            }
            Input::PopoverOutput(output) => match output {
                PopoverOutput::VisibleMonthChanged(month) => self.preload_month(month),
            },
        }
    }
}

impl Applet {
    fn apply_clock_state(&mut self, state: ClockState) {
        self.label = format::label(&self.config.label_format, &state);
        self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
        self.clock_state = state.clone();
        if self.popover.widget().is_visible() {
            self.popover.emit(PopoverInput::UpdateClock(state));
        }
    }

    fn apply_calendar_state(&mut self, state: CalendarState) {
        self.calendar_state = state.clone();
        if self.popover.widget().is_visible() {
            self.popover.emit(PopoverInput::UpdateCalendar(state));
        }
    }

    fn sync_popover(&self) {
        self.popover
            .emit(PopoverInput::UpdateClock(self.clock_state.clone()));
        self.popover
            .emit(PopoverInput::UpdateCalendar(self.calendar_state.clone()));
    }

    fn configure_clock(&self) {
        let service = self.clock.clone();
        let config = self.config.clock_config();
        relm4::spawn(async move {
            if let Err(error) = service
                .send(ServiceCommand::Command(clock::Command::Configure(config)))
                .await
            {
                tracing::warn!(%error, "failed to configure clock service");
            }
        });
    }

    fn preload_month(&self, month: MonthKey) {
        let service = self.calendar.clone();
        relm4::spawn(async move {
            if let Err(error) = service
                .send(ServiceCommand::Command(
                    calendar_events::Command::PreloadAround(month),
                ))
                .await
            {
                tracing::warn!(%error, "failed to preload calendar events");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.clock_cancel.cancel();
        self.calendar_cancel.cancel();
    }
}

fn subscribe_clock(
    service: &clock::ClockHandle,
    cancel: CancellationToken,
    sender: &ComponentSender<Applet>,
) {
    let service = service.clone();
    let sender = sender.clone();
    relm4::spawn(async move {
        let mut sub = service.subscribe();
        sender.input(Input::ClockStateChanged(sub.borrow().clone()));
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = sub.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    sender.input(Input::ClockStateChanged(sub.borrow().clone()));
                }
            }
        }
    });
}

fn subscribe_calendar(
    service: &calendar_events::CalendarEventsHandle,
    cancel: CancellationToken,
    sender: &ComponentSender<Applet>,
) {
    let service = service.clone();
    let sender = sender.clone();
    relm4::spawn(async move {
        let mut sub = service.subscribe();
        sender.input(Input::CalendarStateChanged(sub.borrow().clone()));
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = sub.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    sender.input(Input::CalendarStateChanged(sub.borrow().clone()));
                }
            }
        }
    });
}
