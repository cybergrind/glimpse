mod clock;
mod spacer;
mod tray;

use std::sync::Arc;

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
    Tray(Controller<tray::Tray>),
    Spacer(Controller<Spacer>),
}

impl AppletController {
    pub fn widget(&self) -> gtk::Widget {
        match self {
            AppletController::Clock(c) => c.widget().clone().upcast(),
            AppletController::Tray(c) => c.widget().clone().upcast(),
            AppletController::Spacer(c) => c.widget().clone().upcast(),
        }
    }
}

pub fn create_applet(
    applet_config: Option<&AppletConfig>,
    name: &str,
    _dbus: Arc<zbus::Connection>,
) -> Option<AppletController> {
    let applet_type = applet_config.map(|c| c.extends.as_str()).unwrap_or(name);
    match applet_type {
        "clock" => {
            let config: ClockConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = Clock::builder().launch(ClockInit { config }).detach();
            Some(AppletController::Clock(applet))
        }
        "tray" => {
            let config: tray::TrayConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = tray::Tray::builder()
                .launch(tray::TrayInit { config })
                .detach();
            Some(AppletController::Tray(applet))
        }
        "spacer" => Some(AppletController::Spacer(
            Spacer::builder().launch(()).detach(),
        )),
        _ => None,
    }
}
