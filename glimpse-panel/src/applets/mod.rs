mod audio;
mod battery;
mod bluetooth;
mod clock;
mod network;
mod power;
mod session;
mod spacer;
mod tray;
mod weather;

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
    Audio(Controller<audio::Audio>),
    Battery(Controller<battery::Battery>),
    Bluetooth(Controller<bluetooth::Bluetooth>),
    Clock(Controller<Clock>),
    Network(Controller<network::Network>),
    Power(Controller<power::Power>),
    Tray(Controller<tray::Tray>),
    Weather(Controller<weather::Weather>),
    Session(Controller<session::Session>),
    Spacer(Controller<Spacer>),
}

impl AppletController {
    pub fn widget(&self) -> gtk::Widget {
        match self {
            AppletController::Audio(c) => c.widget().clone().upcast(),
            AppletController::Battery(c) => c.widget().clone().upcast(),
            AppletController::Bluetooth(c) => c.widget().clone().upcast(),
            AppletController::Clock(c) => c.widget().clone().upcast(),
            AppletController::Network(c) => c.widget().clone().upcast(),
            AppletController::Power(c) => c.widget().clone().upcast(),
            AppletController::Tray(c) => c.widget().clone().upcast(),
            AppletController::Weather(c) => c.widget().clone().upcast(),
            AppletController::Session(c) => c.widget().clone().upcast(),
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
    let applet_type = applet_config
        .map(|c| c.extends.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(name);
    match applet_type {
        "audio" => {
            let client = client.clone()?;
            let config: audio::AudioConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = audio::Audio::builder()
                .launch(audio::AudioInit { config, client })
                .detach();
            Some(AppletController::Audio(applet))
        }
        "bluetooth" => {
            let client = client.clone()?;
            let config: bluetooth::BluetoothConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = bluetooth::Bluetooth::builder()
                .launch(bluetooth::BluetoothInit { config, client })
                .detach();
            Some(AppletController::Bluetooth(applet))
        }
        "network" => {
            let client = client.clone()?;
            let config: network::NetworkConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = network::Network::builder()
                .launch(network::NetworkInit { config, client })
                .detach();
            Some(AppletController::Network(applet))
        }
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
            let applet = Clock::builder()
                .launch(ClockInit { config })
                .detach();
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
            let client = client.clone()?;
            let config: tray::TrayConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = tray::Tray::builder()
                .launch(tray::TrayInit { config, client })
                .detach();
            Some(AppletController::Tray(applet))
        }
        "weather" => {
            let config: weather::WeatherConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = weather::Weather::builder()
                .launch(config)
                .detach();
            Some(AppletController::Weather(applet))
        }
        "session" => {
            let client = client.clone()?;
            let config: session::SessionConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = session::Session::builder()
                .launch(session::SessionInit { config, client })
                .detach();
            Some(AppletController::Session(applet))
        }
        "spacer" => Some(AppletController::Spacer(
            Spacer::builder().launch(()).detach(),
        )),
        _ => None,
    }
}
