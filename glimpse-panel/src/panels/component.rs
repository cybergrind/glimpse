use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::prelude::*,
    gtk::{self},
};

use crate::config::{PanelConfig, PanelPosition};

pub struct Panel {}

pub struct Init {
    pub config: PanelConfig,
}

#[derive(Debug)]
pub enum Input {}

#[relm4::component(pub)]
impl SimpleComponent for Panel {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        gtk::Window {
            set_decorated: false,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        Self::setup_layer_shell(&root, &init.config);
        root.add_css_class("panel");

        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        root.set_child(Some(&hbox));

        let model = Panel {};
        let widgets = view_output!();
        root.present();
        ComponentParts { model, widgets }
    }
}

impl Panel {
    fn setup_layer_shell(window: &gtk::Window, config: &PanelConfig) {
        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.set_namespace("glimpse-panel");
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        window.auto_exclusive_zone_enable();
        window.set_height_request(config.height);
        window.set_margin(Edge::Left, config.margin.left);
        window.set_margin(Edge::Right, config.margin.right);
        window.set_margin(Edge::Top, config.margin.top);
        window.set_margin(Edge::Bottom, config.margin.bottom);

        match config.position {
            PanelPosition::Top => {
                window.set_anchor(Edge::Top, true);
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Right, true);
            }
            PanelPosition::Bottom => {
                window.set_anchor(Edge::Bottom, true);
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Right, true);
            }
        }
    }
}
