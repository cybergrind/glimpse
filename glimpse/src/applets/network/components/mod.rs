mod hero;
mod prompt_dialog;
mod vpn_section;
mod wifi_section;
mod wired_row;
mod wired_section;

use std::rc::Rc;

pub use hero::{NetworkHero, NetworkHeroInput};
pub use prompt_dialog::{NetworkPromptDialog, NetworkPromptDialogInit, NetworkPromptDialogInput, NetworkPromptDialogOutput};
pub use vpn_section::{VpnSection, VpnSectionInput};
pub use wifi_section::{WifiSection, WifiSectionInput};
pub use wired_section::{WiredSection, WiredSectionInput};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkAction {
    ToggleWifi(bool),
    ConnectWifi { ssid: String, path: String },
    ConnectSaved { uuid: String },
    Disconnect { uuid: String },
    Forget { uuid: String },
}

pub type NetworkActionSender = Rc<dyn Fn(NetworkAction)>;
