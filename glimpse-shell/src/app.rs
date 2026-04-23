use std::collections::HashMap;

use crate::{
    config::{Config, ConfigEvent, watch_for_config_changes},
    panels,
    services::framework::{Control, ServiceRuntime, Services},
    theme::{self, ThemeState},
};
use adw::gdk::{self, prelude::DisplayExt, prelude::MonitorExt};
use gio::prelude::ListModelExt;
use glib::object::CastNone;
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};
use tokio::sync::mpsc;

pub struct AppInit {
    pub config: Config,
    pub dbus: crate::dbus::Dbus,
}

#[derive(Debug)]
pub enum Input {
    ConfigChanged(Config),
    ThemeReload,
    MonitorsChanged,
}

pub struct App {
    config: Config,
    services: ServiceRuntime,
    theme: ThemeState,
    panels: Vec<PanelState>,
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
        init: Self::Init,
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

        if let Some(display) = gdk::Display::default() {
            let monitor_sender = sender.clone();
            let _ = monitor_sender.input(Input::MonitorsChanged);
            display.monitors().connect_items_changed(move |_, _, _, _| {
                let _ = monitor_sender.input(Input::MonitorsChanged);
            });
        }

        let theme = ThemeState::install(&init.config);

        let (theme_tx, mut theme_rx) = mpsc::channel::<()>(1);
        relm4::spawn(async move {
            theme::watch_user_themes(theme_tx).await;
        });

        let theme_sender = sender.clone();
        relm4::spawn(async move {
            while theme_rx.recv().await.is_some() {
                let _ = theme_sender.input(Input::ThemeReload);
            }
        });

        let services = ServiceRuntime::new(init.dbus);
        services.broadcast(Control::Start(init.config.clone()));

        let widgets = view_output!();
        let model = App {
            config: init.config,
            services,
            theme,
            panels: vec![],
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
                self.theme.reload(&config);
                self.reconcile_panels(&config.panels);
                self.config = config;
            }
            Input::ThemeReload => {
                tracing::info!("theme file changed, reloading");
                self.theme.reload(&self.config);
            }
            Input::MonitorsChanged => {
                tracing::info!("monitors changed, reconciling panels");
                let configs = self.config.panels.clone();
                self.reconcile_panels(&configs);
            }
        }
    }
}

impl App {
    fn reconcile_panels(&mut self, new_configs: &[panels::Config]) {
        let services = self.services.handles();
        let monitors = list_gdk_monitors();

        let mut existing: HashMap<PanelKey, PanelState> = self
            .panels
            .drain(..)
            .map(|state| (state.key.clone(), state))
            .collect();

        let mut new_panels: Vec<PanelState> = Vec::new();
        for (index, cfg) in new_configs.iter().enumerate() {
            for monitor in &monitors {
                let connector = monitor_connector(monitor);
                if let Some(target) = cfg.monitor.as_deref() {
                    if connector.as_deref() != Some(target) {
                        continue;
                    }
                }
                let key = PanelKey {
                    index,
                    monitor: connector.clone().unwrap_or_default(),
                    position: cfg.position.clone(),
                };
                let state = match existing.remove(&key) {
                    Some(state) => {
                        state
                            .controller
                            .emit(panels::Input::Reconfigure(cfg.clone()));
                        state
                    }
                    None => build_panel(index, cfg.clone(), services.clone(), monitor.clone()),
                };
                new_panels.push(state);
            }
        }
        self.panels = new_panels;

        for (key, state) in existing.drain() {
            state.controller.widget().destroy();
            tracing::debug!(
                ?key.position,
                index = key.index,
                monitor = %key.monitor,
                "panel removed"
            );
        }
    }
}

#[derive(PartialEq, Clone, Eq, Hash)]
struct PanelKey {
    index: usize,
    monitor: String,
    position: panels::Position,
}

struct PanelState {
    pub key: PanelKey,
    pub controller: Controller<panels::Panel>,
}

fn list_gdk_monitors() -> Vec<gdk::Monitor> {
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let model = display.monitors();
    (0..model.n_items())
        .filter_map(|i| model.item(i).and_downcast::<gdk::Monitor>())
        .collect()
}

fn monitor_connector(monitor: &gdk::Monitor) -> Option<String> {
    monitor.connector().map(|s| s.to_string())
}

fn build_panel(
    index: usize,
    config: panels::Config,
    services: Services,
    monitor: gdk::Monitor,
) -> PanelState {
    let key = PanelKey {
        index,
        monitor: monitor_connector(&monitor).unwrap_or_default(),
        position: config.position.clone(),
    };
    let controller = panels::Panel::builder()
        .launch(panels::Init {
            config,
            services: services.clone(),
            monitor: Some(monitor),
        })
        .detach();
    PanelState { key, controller }
}
