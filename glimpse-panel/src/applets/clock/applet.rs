use std::time::Duration;

use relm4::{
    Component, ComponentController, Controller,
    gtk::{self, glib, prelude::*},
};

use crate::applets::{
    Applet,
    clock::{clock, config::ClockConfig, popover},
};

pub struct ClockApplet {
    applet: Controller<clock::ClockFace>,
    popover: Controller<popover::Popover>,
    source_id: Option<glib::SourceId>,
}

impl Applet for ClockApplet {
    fn widget(&self) -> gtk::Widget {
        self.applet.widget().clone().upcast()
    }

    fn on_left_click(&self) {
        self.popover.emit(popover::Input::Open);
    }
}
impl ClockApplet {
    pub fn new(config: ClockConfig) -> Self {
        let source_id = glib::timeout_add_local(Duration::from_secs(1), move || {
            println!("TICK");
            glib::ControlFlow::Continue
        });

        let timezones = config.timezones.clone();
        let applet = clock::ClockFace::builder()
            .launch(clock::Init { config })
            .detach();

        let popover = popover::Popover::builder()
            .launch(popover::Init {
                timezones: timezones,
                parent: applet.widget().clone().upcast(),
            })
            .detach();

        Self {
            applet,
            popover,
            source_id: Some(source_id),
        }
    }
}

impl Drop for ClockApplet {
    fn drop(&mut self) {
        if let Some(source_id) = self.source_id.take() {
            source_id.remove();
        }
    }
}
