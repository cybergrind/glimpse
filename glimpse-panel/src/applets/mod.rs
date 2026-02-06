mod clock;
mod component;
mod spacer;

use relm4::gtk::{self};

use crate::{
    applets::{
        clock::config::ClockConfig,
        component::{MouseButton, ScrollDirection},
    },
    config::AppletConfig,
};
use clock::ClockApplet;
pub use component::{AppletHost, AppletHostInit};
use spacer::Spacer;

pub trait Applet {
    fn widget(&self) -> gtk::Widget;
    fn on_scroll(&self, direction: ScrollDirection) {
        match direction {
            ScrollDirection::Up => self.on_scroll_up(),
            ScrollDirection::Down => self.on_scroll_down(),
        }
    }
    fn on_scroll_up(&self) {}
    fn on_scroll_down(&self) {}

    fn on_click(&self, button: MouseButton) {
        match button {
            MouseButton::Left => self.on_left_click(),
            MouseButton::Right => self.on_right_click(),
            MouseButton::Middle => self.on_middle_click(),
        }
    }
    fn on_left_click(&self) {}
    fn on_right_click(&self) {}
    fn on_middle_click(&self) {}
}

pub struct AppletInstance {
    pub applet: Box<dyn Applet>,
}

impl AppletInstance {
    pub fn new(applet: impl Applet + 'static) -> Self {
        Self {
            applet: Box::new(applet),
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        self.applet.widget()
    }
}

pub fn create_applet(config: Option<&AppletConfig>, name: &str) -> Option<AppletInstance> {
    let applet_type = config.map(|c| c.extends.as_str()).unwrap_or(name);

    match applet_type {
        "clock" => {
            let clock_config: ClockConfig = config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            Some(AppletInstance::new(ClockApplet::new(clock_config)))
        }
        "spacer" => Some(AppletInstance::new(Spacer::new())),
        _ => {
            tracing::warn!("unknown applet type: {}", applet_type);
            None
        }
    }
}
