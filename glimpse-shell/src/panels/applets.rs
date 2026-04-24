use relm4::{
    Component, ComponentController, Controller,
    gtk::{self, glib::object::Cast},
};
use serde::Deserialize;

use crate::{applets::battery, panels::PanelSection, services::framework::Services};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AppletType {
    Battery,
}

impl AppletType {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "battery" => Some(Self::Battery),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppletConfig {
    pub extends: Option<AppletType>,
    #[serde(flatten)]
    pub settings: toml::Value,
}

impl Default for AppletConfig {
    fn default() -> Self {
        Self {
            extends: None,
            settings: toml::Value::Table(toml::map::Map::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppletKey {
    pub section: PanelSection,
    pub slot: usize,
    pub name: String,
    pub applet_type: AppletType,
}

#[derive(Debug, Clone)]
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

impl AppletController {
    pub fn widget(&self) -> gtk::Widget {
        match self {
            Self::Battery(controller) => controller.widget().clone().upcast(),
        }
    }
}

pub fn create_applet(blueprint: AppletBlueprint, services: Services) -> Option<AppletController> {
    let _ = services;

    match blueprint.applet_type {
        AppletType::Battery => Some(AppletController::Battery(
            battery::Applet::builder()
                .launch(battery::Init {
                    config: battery::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
    }
}
