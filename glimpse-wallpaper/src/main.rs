mod config;
mod heic;
mod monitor;
mod niri;
mod widgets;

use std::{collections::HashMap, rc::Rc};

use gtk4 as gtk;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;
use tracing::{info, warn};

use config::{WallpaperConfig, load_config, config_file_path};
use monitor::{MonitorWindow, MonitorWindowInit, MonitorWindowMsg, open_all_monitors, connector_name, start_config_watcher};

// ── App component ─────────────────────────────────────────────────────────────

struct WallpaperApp {
    config: Rc<WallpaperConfig>,
    monitors: HashMap<String, Controller<MonitorWindow>>,
}

#[derive(Debug)]
enum AppMsg {
    WorkspaceChanged { output: String, index: u8 },
    MonitorChanged,
    ReloadConfig,
}

impl SimpleComponent for WallpaperApp {
    type Init = WallpaperConfig;
    type Input = AppMsg;
    type Output = ();
    type Root = gtk::Window;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Window::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Relm4 calls root.present() after init() returns, so we must hide it
        // on the next idle tick rather than calling root.hide() directly here.
        let root_ref = root.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(w) = root_ref.upgrade() {
                w.hide();
            }
        });

        let config = Rc::new(init);
        let display = gdk::Display::default().expect("no GDK display");
        let monitors = open_all_monitors(&display, &config);

        let model = WallpaperApp { config, monitors };

        // Monitor hot-plug: send MonitorChanged whenever the list changes.
        let plug_sender = sender.input_sender().clone();
        display.monitors().connect_items_changed(move |_, _, _, _| {
            plug_sender.send(AppMsg::MonitorChanged).ok();
        });

        // Niri workspace watcher: calls on_change from a background thread.
        let niri_sender = sender.input_sender().clone();
        niri::start_workspace_watcher(move |output, index| {
            niri_sender.send(AppMsg::WorkspaceChanged { output, index }).ok();
        });

        // Config file watcher: fires on_change from the notify thread.
        if let Some(config_path) = config_file_path() {
            let cfg_sender = sender.input_sender().clone();
            start_config_watcher(config_path, move || {
                cfg_sender.send(AppMsg::ReloadConfig).ok();
            });
        }

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: AppMsg, _sender: ComponentSender<Self>) {
        match msg {
            AppMsg::WorkspaceChanged { output, index } => {
                if let Some(ctrl) = self.monitors.get(&output) {
                    ctrl.emit(MonitorWindowMsg::SwitchWorkspace(index));
                }
            }

            AppMsg::MonitorChanged => {
                let display = gdk::Display::default().expect("no GDK display");
                let monitors = display.monitors();

                let mut live: HashMap<String, gdk::Monitor> = HashMap::new();
                for i in 0..monitors.n_items() {
                    if let Some(mon) = monitors.item(i).and_downcast::<gdk::Monitor>() {
                        live.insert(connector_name(&mon), mon);
                    }
                }

                let removed: Vec<String> = self
                    .monitors
                    .keys()
                    .filter(|k| !live.contains_key(*k))
                    .cloned()
                    .collect();

                for connector in &removed {
                    info!("wallpaper: monitor disconnected: {connector}");
                    if let Some(ctrl) = self.monitors.remove(connector) {
                        ctrl.widget().close();
                    }
                }

                for (name, monitor) in &live {
                    if !self.monitors.contains_key(name) {
                        info!("wallpaper: monitor connected: {name}");
                        let ctrl = MonitorWindow::builder()
                            .launch(MonitorWindowInit {
                                monitor: monitor.clone(),
                                config: (*self.config).clone(),
                            })
                            .detach();
                        self.monitors.insert(name.clone(), ctrl);
                    }
                }
            }

            AppMsg::ReloadConfig => {
                match load_config() {
                    Ok(config) => {
                        info!("wallpaper: config reloaded — rebuilding windows");
                        for (_, ctrl) in self.monitors.drain() {
                            ctrl.widget().close();
                        }
                        self.config = Rc::new(config);
                        let display = gdk::Display::default().expect("no GDK display");
                        self.monitors = open_all_monitors(&display, &self.config);
                    }
                    Err(e) => warn!("wallpaper: config reload failed: {e}"),
                }
            }
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_env("GLIMPSE_WALLPAPER_LOG_LEVEL")
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("starting glimpse-wallpaper");

    let config = match load_config() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("failed to load config: {e}");
            return;
        }
    };

    RelmApp::new("me.aresa.GlimpseWallpaper").run::<WallpaperApp>(config);
}
