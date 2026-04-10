mod hero;
mod vpn_section;
mod wifi_section;
mod wired_section;

use std::rc::Rc;

pub use hero::NetworkHero;
pub use vpn_section::VpnSection;
pub use wifi_section::WifiSection;
pub use wired_section::WiredSection;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkCommand {
    ToggleWifi(bool),
    ConnectWifi { ssid: String },
    ConnectSaved { uuid: String },
    Disconnect { uuid: String },
    Forget { uuid: String },
    OpenSettings,
}

pub type NetworkCommandSender = Rc<dyn Fn(NetworkCommand)>;
