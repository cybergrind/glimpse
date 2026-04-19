use crate::{
    config::{Config, ConfigEvent, watch_for_config_changes},
    services::framework::{Control, Services},
};
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::{ComponentParts, ComponentSender, SimpleComponent};
use tokio::sync::mpsc;

pub struct AppInit {
    pub config: Config,
    pub services: Services,
}

#[derive(Debug)]
pub enum Input {
    ConfigChanged(Config),
}

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
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.init_layer_shell();
        root.set_layer(gtk4_layer_shell::Layer::Background);
        root.set_namespace("glimpse-shell");
        root.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        root.set_default_size(-1, -1);
        root.set_opacity(0.0);

        let (config_tx, mut config_rx) = mpsc::channel(1);
        relm4::spawn(async move {
            watch_for_config_changes(config_tx).await;
        });

        let config_sender = sender.clone();
        relm4::spawn(async move {
            loop {
                match config_rx.recv().await {
                    Some(message) => match message {
                        ConfigEvent::Changed(config) => {
                            let _ = config_sender.input(Input::ConfigChanged(config));
                        }
                    },
                    None => break,
                }
            }
        });

        let widgets = view_output!();
        let model = App {
            config: config.config,
            services: config.services,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Input::ConfigChanged(config) => {
                if self.config == config {
                    return;
                }

                tracing::info!("app config changed");
                self.services
                    .broadcast(Control::Reconfigure(config.clone()));
                self.config = config;
            }
        }
    }
}
