mod audio;
mod battery;
mod bluetooth;
mod brightness;
mod clock;
mod exec;
mod keyboard;
mod mpris;
mod network;
pub(crate) mod notifications;
mod pager;
mod power;
mod privacy;
pub mod registry;
mod session;
mod tray;
mod weather;

use clock::Clock;
use exec::Exec;
use relm4::{
    ComponentController, Controller,
    gtk::{self, glib::object::Cast},
};

pub enum AppletController {
    Audio(Controller<audio::Audio>),
    Battery(Controller<battery::Battery>),
    Bluetooth(Controller<bluetooth::Bluetooth>),
    Brightness(Controller<brightness::Brightness>),
    Clock(Controller<Clock>),
    Exec(Controller<Exec>),
    Keyboard(Controller<keyboard::Keyboard>),
    Mpris(Controller<mpris::Mpris>),
    Network(Controller<network::Network>),
    Notifications(Controller<notifications::Notifications>),
    Power(Controller<power::Power>),
    Privacy(Controller<privacy::Privacy>),
    Tray(Controller<tray::Tray>),
    Weather(Controller<weather::Weather>),
    Session(Controller<session::Session>),
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
            AppletController::Pager(c) => c.widget().clone().upcast(),
        }
    }
}

pub use registry::{create_applet, reconfigure_applet};
