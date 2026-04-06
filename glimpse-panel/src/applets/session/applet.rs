use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use super::config::SessionConfig;
use super::popover::{SessionPopover, SessionPopoverInit, SessionPopoverInput};

pub struct Session {
    popover: Controller<SessionPopover>,
}

pub struct SessionInit {
    pub config: SessionConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum SessionMsg {
    TogglePopover,
}

#[relm4::component(pub)]
impl Component for Session {
    type Init = SessionInit;
    type Input = SessionMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "session",
            #[watch]
            set_tooltip_text: Some("Session"),

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(SessionMsg::TogglePopover);
                }
            },

            gtk::Label {
                set_label: &std::env::var("USER").unwrap_or_else(|_| "user".into()),
                add_css_class: "session-label",
            },
        }
    }

    fn init(
        init: Self::Init, root: Self::Root, sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = SessionPopover::builder()
            .launch(SessionPopoverInit {
                parent: root.clone(),
                client: init.client.clone(),
                config: init.config,
            })
            .detach();

        let model = Session { popover };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            SessionMsg::TogglePopover => {
                self.popover.emit(SessionPopoverInput::Toggle);
            }
        }
    }
}
