mod clock;
mod spacer;

use relm4::{
    Component, ComponentController, Controller,
    gtk::{self, prelude::*},
};

use crate::config::AppletConfig;
use clock::{ClockApplet, ClockConfig};
use spacer::Spacer;

pub enum AppletInstance {
    Clock(Controller<ClockApplet>),
    Spacer(Controller<Spacer>),
}

impl AppletInstance {
    pub fn widget(&self) -> gtk::Widget {
        match self {
            Self::Clock(c) => c.widget().clone().upcast(),
            Self::Spacer(c) => c.widget().clone().upcast(),
        }
    }
}

pub fn create_applet(config: Option<&AppletConfig>, name: &str) -> Option<AppletInstance> {
    let applet_type = config.map(|c| c.extends.as_str()).unwrap_or(name);

    match applet_type {
        "clock" => {
            let clock_config: ClockConfig = config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let controller = ClockApplet::builder().launch(clock_config).detach();
            Some(AppletInstance::Clock(controller))
        }
        "spacer" => {
            let controller = Spacer::builder().launch(()).detach();
            Some(AppletInstance::Spacer(controller))
        }
        _ => {
            tracing::warn!("unknown applet type: {}", applet_type);
            None
        }
    }
}
