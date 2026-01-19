use std::time::Duration;

use relm4::gtk::{self, glib, prelude::*};
use serde::Deserialize;

use crate::applets::Applet;

#[derive(Debug, Clone, Deserialize)]
pub struct ClockConfig {
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "%H:%M".to_string()
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
        }
    }
}

pub struct ClockApplet {
    widget: gtk::Box,
    popover: gtk::Popover,
    source_id: Option<glib::SourceId>,
}

impl Applet for ClockApplet {
    fn widget(&self) -> gtk::Widget {
        self.widget.clone().upcast()
    }

    fn on_left_click(&self) {
        self.popover.popup();
    }
}

impl ClockApplet {
    pub fn new(config: ClockConfig) -> Self {
        let time = chrono::Local::now().format(&config.format).to_string();
        let label = gtk::Label::new(Some(&time));
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        hbox.add_css_class("clock");
        hbox.append(&label);

        let format = config.format.clone();
        let label_clone = label.clone();

        let source_id = glib::timeout_add_local(Duration::from_secs(1), move || {
            let time = chrono::Local::now().format(&format).to_string();
            label_clone.set_label(&time);
            glib::ControlFlow::Continue
        });

        let popover = create_popover(hbox.clone());

        Self {
            popover,
            widget: hbox,
            source_id: Some(source_id),
        }
    }
}

impl Drop for ClockApplet {
    fn drop(&mut self) {
        if let Some(source_id) = self.source_id.take() {
            source_id.remove();
        }
    }
}

fn create_popover(parent: gtk::Box) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(&parent);

    let calendar = gtk::Calendar::new();
    popover.set_child(Some(&calendar));
    popover
}
