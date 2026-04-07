mod audio;
mod battery;
mod bluetooth;
mod brightness;
mod clock;
mod exec;
mod keyboard;
mod mpris;
mod network;
mod notifications;
mod power;
mod privacy;
mod session;
mod spacer;
mod tray;
mod weather;
mod pager;

use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, Controller,
    gtk::{self, glib::object::Cast},
};

use crate::{
    applets::{
        clock::{Clock, ClockConfig, ClockInit},
        exec::{Exec, ExecConfig, ExecInit},
    },
    config::AppletConfig,
};
use spacer::Spacer;

pub enum AppletController {
    Audio(Controller<audio::Audio>),
    Battery(Controller<battery::Battery>),
    Bluetooth(Controller<bluetooth::Bluetooth>),
    Brightness(Controller<brightness::Brightness>),
    Clock(Controller<Clock>),
    Exec(Controller<exec::Exec>),
    Keyboard(Controller<keyboard::Keyboard>),
    Mpris(Controller<mpris::Mpris>),
    Network(Controller<network::Network>),
    Notifications(Controller<notifications::Notifications>),
    Power(Controller<power::Power>),
    Privacy(Controller<privacy::Privacy>),
    Tray(Controller<tray::Tray>),
    Weather(Controller<weather::Weather>),
    Session(Controller<session::Session>),
    Spacer(Controller<Spacer>),
    Pager(Controller<pager::Pager>),
}

impl AppletController {
    pub fn widget(&self) -> gtk::Widget {
        match self {
            AppletController::Audio(c) => c.widget().clone().upcast(),
            AppletController::Battery(c) => c.widget().clone().upcast(),
            AppletController::Bluetooth(c) => c.widget().clone().upcast(),
            AppletController::Brightness(c) => c.widget().clone().upcast(),
            AppletController::Clock(c) => c.widget().clone().upcast(),
            AppletController::Exec(c) => c.widget().clone().upcast(),
            AppletController::Keyboard(c) => c.widget().clone().upcast(),
            AppletController::Mpris(c) => c.widget().clone().upcast(),
            AppletController::Network(c) => c.widget().clone().upcast(),
            AppletController::Notifications(c) => c.widget().clone().upcast(),
            AppletController::Power(c) => c.widget().clone().upcast(),
            AppletController::Privacy(c) => c.widget().clone().upcast(),
            AppletController::Tray(c) => c.widget().clone().upcast(),
            AppletController::Weather(c) => c.widget().clone().upcast(),
            AppletController::Session(c) => c.widget().clone().upcast(),
            AppletController::Spacer(c) => c.widget().clone().upcast(),
            AppletController::Pager(c) => c.widget().clone().upcast(),
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
    tracing::debug!(name, applet_type, "creating applet");
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
        "brightness" => {
            let client = client.clone()?;
            let config: brightness::BrightnessConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = brightness::Brightness::builder()
                .launch(brightness::BrightnessInit { config, client })
                .detach();
            Some(AppletController::Brightness(applet))
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
        "mpris" => {
            let client = client.clone()?;
            let config: mpris::MprisConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = mpris::Mpris::builder()
                .launch(mpris::MprisInit { config, client })
                .detach();
            Some(AppletController::Mpris(applet))
        }
        "exec" => {
            let config: ExecConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            if config.command.is_empty() {
                tracing::error!(name, "exec applet requires a non-empty command");
                return None;
            }
            let applet = Exec::builder()
                .launch(ExecInit {
                    name: name.to_string(),
                    config,
                })
                .detach();
            Some(AppletController::Exec(applet))
        }
        "notifications" => {
            let client = client.clone()?;
            let config: notifications::NotificationsConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = notifications::Notifications::builder()
                .launch(notifications::NotificationsInit { config, client })
                .detach();
            Some(AppletController::Notifications(applet))
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
                .launch(ClockInit {
                    config,
                    client: client.clone(),
                })
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
        "privacy" => {
            let client = client.clone()?;
            let config: privacy::PrivacyConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = privacy::Privacy::builder()
                .launch(privacy::PrivacyInit { config, client })
                .detach();
            Some(AppletController::Privacy(applet))
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
            let applet = weather::Weather::builder().launch(config).detach();
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
        "pager" => {
            let config: pager::PagerConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = pager::Pager::builder()
                .launch(pager::PagerInit { config })
                .detach();
            Some(AppletController::Pager(applet))
        }
        "keyboard" => {
            let config: keyboard::KeyboardConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            let applet = keyboard::Keyboard::builder()
                .launch(keyboard::KeyboardInit { config })
                .detach();
            Some(AppletController::Keyboard(applet))
        }
        "spacer" => Some(AppletController::Spacer(
            Spacer::builder().launch(()).detach(),
        )),
        _ => None,
    }
}
