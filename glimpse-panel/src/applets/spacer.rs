use relm4::gtk::{self, prelude::*};

use crate::applets::Applet;

pub struct Spacer {
    widget: gtk::Box,
}

impl Spacer {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        widget.set_hexpand(true);
        Self { widget }
    }
}

impl Applet for Spacer {
    fn widget(&self) -> gtk::Widget {
        self.widget.clone().upcast()
    }
}
