use chrono::Local;
use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{
        self,
        prelude::{GestureSingleExt, OrientableExt, WidgetExt},
    },
};
use std::sync::Arc;
use std::time::Duration;

use crate::applets::clock::{
    config::ClockConfig,
    popover::{Popover, PopoverInit, PopoverInput},
};

pub struct Clock {
    pub value: String,
    pub config: ClockConfig,
    popover: Controller<Popover>,
}

pub struct ClockInit {
    pub config: ClockConfig,
    pub client: Option<Arc<Client>>,
}

#[derive(Debug)]
pub enum ClockInput {
    Tick,
    TogglePopover,
}

#[derive(Debug)]
pub enum CommandOutput {
    Tick,
}

#[relm4::component(pub)]
impl Component for Clock {
    type Init = ClockInit;
    type Input = ClockInput;
    type Output = ();
    type CommandOutput = CommandOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "applet",
            add_css_class: "clock",
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
        let popover = Popover::builder()
            .launch(PopoverInit {
                parent: root.clone(),
                timezones: init.config.timezones.clone(),
                client: init.client,
            })
            .detach();

        let model = Clock {
            popover,
            value: String::new(),
            config: init.config,
        };
        let widgets = view_output!();

        // Clock always uses a local timer — it only needs chrono::Local::now()
        // to format the time string. No daemon dependency needed.
        sender.command(|out, shutdown| {
            out.send(CommandOutput::Tick).ok();
            shutdown
                .register(async move {
                    local_timer(out).await;
                })
                .drop_on_shutdown()
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match input {
            ClockInput::Tick => {
                self.value = Local::now().format(&self.config.format).to_string();
                self.popover.emit(PopoverInput::Tick);
            }
            ClockInput::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
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
            CommandOutput::Tick => sender.input(ClockInput::Tick),
        }
    }
}

async fn local_timer(out: relm4::Sender<CommandOutput>) {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if out.send(CommandOutput::Tick).is_err() {
            break;
        }
    }
}
