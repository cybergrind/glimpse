mod battery;
mod clock;
mod power;
mod spacer;
mod tray;

use std::sync::Arc;

use glimpse_client::Client;
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
    Battery(Controller<battery::Battery>),
    Clock(Controller<Clock>),
    Power(Controller<power::Power>),
    Tray(Controller<tray::Tray>),
    Spacer(Controller<Spacer>),
}

impl AppletController {
    pub fn widget(&self) -> gtk::Widget {
        match self {
            AppletController::Battery(c) => c.widget().clone().upcast(),
            AppletController::Clock(c) => c.widget().clone().upcast(),
            AppletController::Power(c) => c.widget().clone().upcast(),
            AppletController::Tray(c) => c.widget().clone().upcast(),
            AppletController::Spacer(c) => c.widget().clone().upcast(),
        }
    }
}

pub fn create_applet(
    applet_config: Option<&AppletConfig>,
    name: &str,
    dbus: Arc<zbus::Connection>,
    client: Option<Arc<Client>>,
) -> Option<AppletController> {
    let applet_type = applet_config.map(|c| c.extends.as_str()).unwrap_or(name);
    match applet_type {
        "battery" => {
            let client = client.clone()?;
            let config: battery::BatteryConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = battery::Battery::builder()
                .launch(battery::BatteryInit { config, client })
                .detach();
            Some(AppletController::Battery(applet))
        }
        "clock" => {
            let config: ClockConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = Clock::builder().launch(ClockInit { config }).detach();
            Some(AppletController::Clock(applet))
        }
        "power" => {
            let config: power::PowerConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = power::Power::builder()
                .launch(power::PowerInit {
                    config,
                    dbus: dbus.clone(),
                })
                .detach();
            Some(AppletController::Power(applet))
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
