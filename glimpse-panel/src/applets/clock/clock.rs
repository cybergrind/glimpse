use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::applets::clock::config::ClockConfig;

#[derive(Debug)]
pub enum Input {
    Tick,
}

pub struct Init {
    pub config: ClockConfig,
}

pub struct ClockFace {
    format: String,
    value: String,
}

#[relm4::component(pub)]
impl SimpleComponent for ClockFace {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "applet",
            add_css_class: "clock",
            gtk::Label {
                set_label: "Clock"
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        let model = ClockFace {
            format: init.config.format,
            value: String::new(),
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Tick => {
                self.value = chrono::Local::now().format(&self.format).to_string();
            }
        }
    }
}
