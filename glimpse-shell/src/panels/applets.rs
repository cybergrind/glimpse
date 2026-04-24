use relm4::{Component, Controller};
use serde::Deserialize;

use crate::{applets::battery, panels::PanelSection, services::framework::Services};

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AppletType {
    Battery,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AppletConfig {
    pub extends: AppletType,
    #[serde(flatten)]
    pub settings: toml::Value,
}

pub struct AppletKey {
    pub section: PanelSection,
    pub slot: usize,
    pub name: String,
    pub applet_type: AppletType,
}

pub struct AppletBlueprint {
    pub key: AppletKey,
    pub slot: usize,
    pub name: String,
    pub applet_type: AppletType,
    pub config: Option<AppletConfig>,
}
pub enum AppletController {
    Battery(Controller<battery::Applet>),
}

pub fn create_applet(blueprint: AppletBlueprint, services: Services) -> Option<AppletController> {
    match blueprint.applet_type {
        AppletType::Battery => Some(
            battery::Applet::builder()
                .launch(battery::Init {
                    config: battery::Config::from_raw(&blueprint.config),
                })
                .detach(),
        ),
    }
    None
}
