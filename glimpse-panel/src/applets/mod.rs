mod clock;
mod spacer;

use relm4::{
    Component, ComponentController, Controller,
    gtk::{self, glib::object::Cast},
};

use crate::{
    applets::clock::{Clock, ClockConfig, ClockInit},
    config::AppletConfig,
};
use spacer::Spacer;

pub enum AppletController {
    Clock(Controller<Clock>),
    Spacer(Controller<Spacer>),
}

impl AppletController {
    pub fn widget(&self) -> gtk::Widget {
        match self {
            AppletController::Clock(c) => c.widget().clone().upcast(),
            AppletController::Spacer(c) => c.widget().clone().upcast(),
        }
    }
}

pub fn create_applet(applet_config: Option<&AppletConfig>, name: &str) -> Option<AppletController> {
    let applet_type = applet_config.map(|c| c.extends.as_str()).unwrap_or(name);
    match applet_type {
        "clock" => {
            let config: ClockConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = Clock::builder().launch(ClockInit { config }).detach();
            Some(AppletController::Clock(applet))
        }
        "spacer" => Some(AppletController::Spacer(
            Spacer::builder().launch(()).detach(),
        )),
        _ => None,
    }
}
