use std::collections::HashMap;

use crate::{
    agents::{bluetooth::BluetoothAgentRuntime, network::NetworkAgentRuntime},
    panels,
    prompts::{bluetooth as bluetooth_prompts, network as network_prompts},
    services::framework::{Control, ServiceRuntime, Services},
    theme::{self, ThemeState},
};
use adw::gdk::{self, prelude::DisplayExt, prelude::MonitorExt};
use gio::prelude::ListModelExt;
use glib::object::{Cast, CastNone};
use glimpse_core::{
    Config, ConfigEvent, PanelConfig, services::theme::State as ThemeServiceState,
    watch_for_config_changes,
};
use gtk4::prelude::{GtkWindowExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct AppInit {
    pub config: Config,
    pub dbus: glimpse_core::dbus::Dbus,
}

#[derive(Debug)]
pub enum Input {
    ConfigChanged(Config),
    ThemeReload,
    ThemeChanged(ThemeServiceState),
    MonitorsChanged,
}

pub struct App {
    config: Config,
    services: ServiceRuntime,
    theme: ThemeState,
    panels: Vec<PanelState>,
    network_prompt_host: Controller<network_prompts::PromptHost>,
    bluetooth_prompt_host: Controller<bluetooth_prompts::PromptHost>,
    network_agent_cancel: CancellationToken,
    bluetooth_agent_cancel: CancellationToken,
    prompt_fallback_parent: gtk4::Widget,
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

        let config_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            loop {
                match config_rx.recv().await {
                    Some(message) => match message {
                        ConfigEvent::Changed(config) => {
                            if config_sender.send(Input::ConfigChanged(config)).is_err() {
                                break;
                            }
                        }
                    },
                    None => break,
                }
            }
        });

        if let Some(display) = gdk::Display::default() {
            let monitor_sender = sender.input_sender().clone();
            let _ = monitor_sender.send(Input::MonitorsChanged);
            display.monitors().connect_items_changed(move |_, _, _, _| {
                let _ = monitor_sender.send(Input::MonitorsChanged);
            });
        }

        let theme = ThemeState::install(&init.config);

        let (theme_tx, mut theme_rx) = mpsc::channel::<()>(1);
        relm4::spawn(async move {
            theme::watch_user_themes(theme_tx).await;
        });

        let theme_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            while theme_rx.recv().await.is_some() {
                if theme_sender.send(Input::ThemeReload).is_err() {
                    break;
                }
            }
        });

        let system_dbus = init.dbus.system.clone();
        let (network_agent_runtime, network_agent) = NetworkAgentRuntime::new(system_dbus.clone());
        let network_agent_cancel = CancellationToken::new();
        {
            let cancel = network_agent_cancel.clone();
            relm4::spawn(async move {
                network_agent_runtime.run(cancel).await;
            });
        }

        let (bluetooth_agent_runtime, bluetooth_agent) = BluetoothAgentRuntime::new(system_dbus);
        let bluetooth_agent_cancel = CancellationToken::new();
        {
            let cancel = bluetooth_agent_cancel.clone();
            relm4::spawn(async move {
                bluetooth_agent_runtime.run(cancel).await;
            });
        }

        let services = ServiceRuntime::new(init.dbus);
        services.broadcast(Control::Start(init.config.clone()));
        spawn_theme_subscription(services.handles().theme, sender.input_sender().clone());

        let prompt_fallback_parent: gtk4::Widget = root.clone().upcast();

        let network_prompt_host = network_prompts::PromptHost::builder()
            .launch(network_prompts::PromptHostInit {
                agent: network_agent,
                parent: prompt_fallback_parent.clone(),
            })
            .detach();

        let bluetooth_prompt_host = bluetooth_prompts::PromptHost::builder()
            .launch(bluetooth_prompts::PromptHostInit {
                agent: bluetooth_agent,
                parent: prompt_fallback_parent.clone(),
            })
            .detach();

        let widgets = view_output!();
        let model = App {
            config: init.config,
            services,
            theme,
            panels: vec![],
            network_prompt_host,
            bluetooth_prompt_host,
            network_agent_cancel,
            bluetooth_agent_cancel,
            prompt_fallback_parent,
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
                self.reconcile_panels(&config);
                self.config = config;
            }
            Input::ThemeReload => {
                tracing::info!("theme file changed, reloading");
                self.theme.reload(&self.config);
            }
            Input::ThemeChanged(state) => {
                if state.configured_mode != self.config.theme_mode {
                    tracing::debug!(
                        current_configured_mode = ?self.config.theme_mode,
                        stale_configured_mode = ?state.configured_mode,
                        stale_effective_mode = ?state.effective_mode,
                        "ignoring stale theme service state"
                    );
                    return;
                }
                tracing::debug!(
                    configured_mode = ?state.configured_mode,
                    effective_mode = ?state.effective_mode,
                    reason = ?state.reason,
                    "applying theme service state"
                );
                self.theme.apply_effective_mode(state.effective_mode);
            }
            Input::MonitorsChanged => {
                tracing::info!("monitors changed, reconciling panels");
                let config = self.config.clone();
                self.reconcile_panels(&config);
            }
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.network_agent_cancel.cancel();
        self.bluetooth_agent_cancel.cancel();
    }
}

fn spawn_theme_subscription(
    theme: glimpse_core::services::theme::ThemeHandle,
    sender: relm4::Sender<Input>,
) {
    relm4::spawn(async move {
        let mut state_rx = theme.subscribe();
        if sender
            .send(Input::ThemeChanged(*state_rx.borrow()))
            .is_err()
        {
            return;
        }

        loop {
            if state_rx.changed().await.is_err() {
                break;
            }
            if sender
                .send(Input::ThemeChanged(*state_rx.borrow()))
                .is_err()
            {
                break;
            }
        }
    });
}

impl App {
    fn reconcile_panels(&mut self, new_config: &Config) {
        let services = self.services.handles();
        let monitors = list_gdk_monitors();

        let mut existing: HashMap<panels::PanelKey, PanelState> = self
            .panels
            .drain(..)
            .map(|state| (state.key.clone(), state))
            .collect();

        let mut new_panels: Vec<PanelState> = Vec::new();
        for (index, cfg) in new_config.panels.iter().enumerate() {
            for monitor in &monitors {
                let connector = monitor_connector(monitor);
                if let Some(target) = cfg.monitor.as_deref() {
                    if connector.as_deref() != Some(target) {
                        continue;
                    }
                }
                let key = panels::PanelKey {
                    index,
                    monitor: connector.clone().unwrap_or_default(),
                    position: cfg.position.clone(),
                };
                let state = match existing.remove(&key) {
                    Some(state) => {
                        state.controller.emit(panels::Input::Reconfigure(
                            panels::PanelRuntimeConfig {
                                config: cfg.clone(),
                                applet_configs: new_config.applets.clone(),
                            },
                        ));
                        state
                    }
                    None => build_panel(
                        index,
                        cfg.clone(),
                        services.clone(),
                        monitor.clone(),
                        new_config.clone(),
                    ),
                };
                new_panels.push(state);
            }
        }
        self.panels = new_panels;
        self.update_prompt_parent();

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

    fn update_prompt_parent(&self) {
        let parent = self
            .panels
            .first()
            .map(|panel| panel.controller.widget().clone().upcast())
            .unwrap_or_else(|| self.prompt_fallback_parent.clone());

        self.network_prompt_host
            .emit(network_prompts::PromptHostInput::SetParent(parent.clone()));
        self.bluetooth_prompt_host
            .emit(bluetooth_prompts::PromptHostInput::SetParent(parent));
    }
}

struct PanelState {
    pub key: panels::PanelKey,
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
    config: PanelConfig,
    services: Services,
    monitor: gdk::Monitor,
    app_config: Config,
) -> PanelState {
    let key = panels::PanelKey {
        index,
        monitor: monitor_connector(&monitor).unwrap_or_default(),
        position: config.position.clone(),
    };
    let controller = panels::Panel::builder()
        .launch(panels::Init {
            config,
            services: services.clone(),
            monitor: Some(monitor),
            applet_configs: app_config.applets.clone(),
        })
        .detach();
    PanelState { key, controller }
}
