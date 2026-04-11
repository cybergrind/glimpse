use chrono::Local;
use glimpse::{
    calendar::{
        CalendarServiceHandle,
        protocol::{CalendarDate, CalendarServiceCommand, CalendarServiceState},
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
                        let _ = out.send(ClockCommandOutput::CalendarState(state_rx.borrow().clone()));
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
            ClockCommandOutput::CalendarState(state) => sender.input(ClockInput::CalendarState(state)),
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
