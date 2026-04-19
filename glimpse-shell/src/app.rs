use gtk4::prelude::{GtkWindowExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::{ComponentParts, ComponentSender, SimpleComponent};

use crate::{config::Config, services::framework::Services};

pub struct AppInit {
    pub config: Config,
    pub services: Services,
}

#[derive(Debug)]
pub enum Input {}

pub struct App {
    config: Config,
    services: Services,
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = AppInit;
    type Input = Input;
    type Output = ();

    view! {
        adw::ApplicationWindow {
            set_visible: false,
            set_decorated: false,
            set_deletable: false,
            set_resizable: false,
        }
    }

    fn init(
        config: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.init_layer_shell();
        root.set_layer(gtk4_layer_shell::Layer::Background);
        root.set_namespace("glimpse-shell");
        root.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        root.set_default_size(-1, -1);
        root.set_opacity(0.0);

        let widgets = view_output!();
        let model = App {
            config: config.config,
            services: config.services,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {}
}
