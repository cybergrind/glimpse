use std::{
    path::PathBuf,
    time::Duration,
};

use adw::prelude::*;
use gtk4_layer_shell::LayerShell;
use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, CssProvider, gdk::Display},
};

use glimpse::backdrop;
use glimpse::config::Config;
use glimpse::wallpaper;

use crate::{
    applets::notifications::{
        NotificationActionCommand, NotificationPopup, NotificationPopupInit, NotificationsConfig,
    },
    panels,
    providers::dbus::DbusProvider,
    services::{Services, ServicesHandle},
};

pub struct App {
    config: Config,
    theme_css: CssProvider,
    panels: Vec<Controller<panels::Panel>>,
    wallpaper_windows: std::collections::HashMap<String, Controller<wallpaper::MonitorWindow>>,
    backdrop_windows: std::collections::HashMap<String, Controller<backdrop::BackdropWindow>>,
    dbus: DbusProvider,
    services: Services,
    notification_popup: Option<Controller<NotificationPopup>>,
}

#[derive(Debug)]
pub enum Input {
    ConfigChanged(Config),
    CssChanged,
    MonitorsChanged,
    NotificationCommand(NotificationActionCommand),
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = Config;
    type Input = Input;
    type Output = ();

    view! {
        adw::ApplicationWindow {
            set_visible: true,
            set_default_size: (800, 38),
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
        root.set_namespace("glimpse-app");
        root.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        root.set_default_size(1, 1);
        root.set_opacity(0.0);

        let theme_css = CssProvider::new();
        load_css(&theme_css, &config.theme_path());
        if let Some(display) = Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &theme_css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        watch_for_config_changes(sender.clone());

        if let Some(display) = Display::default() {
            let monitor_sender = sender.input_sender().clone();
            display.monitors().connect_items_changed(move |_, _, _, _| {
                monitor_sender.send(Input::MonitorsChanged).ok();
            });
        }

        let dbus = DbusProvider::connect();
        let services = Services::new(dbus.session.clone(), dbus.system.clone());

        let panels = setup_panels(
            &config,
            dbus.session.clone(),
            dbus.system.clone(),
            services.handle.clone(),
        );
        let notification_popup = setup_notification_popup(
            &config,
            services.handle.notifications.clone(),
            sender.clone(),
        );

        let model = App {
            panels,
            wallpaper_windows: std::collections::HashMap::new(),
            backdrop_windows: std::collections::HashMap::new(),
            theme_css,
            config,
            dbus,
            services,
            notification_popup,
        };

        let startup_sender = sender.input_sender().clone();
        gtk::glib::idle_add_local_once(move || {
            let _ = startup_sender.send(Input::MonitorsChanged);
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Input::ConfigChanged(new_config) => {
                for panel in self.panels.drain(..) {
                    panel.widget().close();
                }
                if let Some(popup) = self.notification_popup.take() {
                    popup.widget().close();
                }
                rebuild_background_windows(
                    Display::default(),
                    &new_config,
                    &mut self.wallpaper_windows,
                    &mut self.backdrop_windows,
                );
                self.panels = setup_panels(
                    &new_config,
                    self.dbus.session.clone(),
                    self.dbus.system.clone(),
                    self.services.handle.clone(),
                );
                self.notification_popup = setup_notification_popup(
                    &new_config,
                    self.services.handle.notifications.clone(),
                    _sender.clone(),
                );
                self.config = new_config;
            }
            Input::CssChanged => {
                load_css(&self.theme_css, &self.config.theme_path());
            }
            Input::MonitorsChanged => {
                rebuild_background_windows(
                    Display::default(),
                    &self.config,
                    &mut self.wallpaper_windows,
                    &mut self.backdrop_windows,
                );
            }
            Input::NotificationCommand(command) => {
                let notifications = self.services.handle.notifications.clone();
                relm4::spawn(async move {
                    if let Err(error) = notifications.send(command.into_service_command()).await {
                        tracing::warn!(error = %error, "notifications app: failed to send command");
                    }
                });
            }
        }
    }
}

fn rebuild_background_windows(
    display: Option<Display>,
    config: &Config,
    wallpaper_windows: &mut std::collections::HashMap<String, Controller<wallpaper::MonitorWindow>>,
    backdrop_windows: &mut std::collections::HashMap<String, Controller<backdrop::BackdropWindow>>,
) {
    close_wallpaper_windows(wallpaper_windows);
    close_backdrop_windows(backdrop_windows);

    let Some(display) = display else {
        return;
    };

    *wallpaper_windows = wallpaper::open_all_monitors(&display, &config.wallpaper);
    *backdrop_windows = backdrop::open_all_monitors(&display, &config.backdrop);
}

fn close_wallpaper_windows(
    wallpaper_windows: &mut std::collections::HashMap<String, Controller<wallpaper::MonitorWindow>>,
) {
    for (_, ctrl) in wallpaper_windows.drain() {
        ctrl.widget().close();
    }
}

fn close_backdrop_windows(
    backdrop_windows: &mut std::collections::HashMap<String, Controller<backdrop::BackdropWindow>>,
) {
    for (_, window) in backdrop_windows.drain() {
        window.widget().close();
    }
}

fn setup_panels(
    config: &Config,
    dbus: zbus::Connection,
    system: zbus::Connection,
    services: ServicesHandle,
) -> Vec<Controller<panels::Panel>> {
    let mut panels = vec![];
    for panel_config in &config.panels {
        let panel_init = panels::Init {
            config: panel_config.clone(),
            applet_configs: config.applets.clone(),
            dbus: dbus.clone(),
            system: system.clone(),
            services: services.clone(),
        };
        let panel = panels::Panel::builder().launch(panel_init).detach();
        panels.push(panel);
    }
    panels
}

fn notifications_popup_config(config: &Config) -> Option<NotificationsConfig> {
    for panel in &config.panels {
        for name in &panel.applets {
            let applet_config = config.applets.get(name);
            let applet_type = applet_config
                .map(|c| c.extends.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(name);
            if applet_type != "notifications" {
                continue;
            }

            let popup_config: NotificationsConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            return popup_config.show_popup.then_some(popup_config);
        }
    }

    None
}

fn setup_notification_popup(
    config: &Config,
    service: glimpse::notifications::NotificationsServiceHandle,
    sender: ComponentSender<App>,
) -> Option<Controller<NotificationPopup>> {
    let popup_config = notifications_popup_config(config)?;
    Some(
        NotificationPopup::builder()
            .launch(NotificationPopupInit {
                config: popup_config,
                service,
            })
            .forward(sender.input_sender(), Input::NotificationCommand),
    )
}

fn load_css(provider: &CssProvider, path: &PathBuf) {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.clone());
    if resolved.exists() && resolved.is_file() {
        provider.load_from_path(&resolved);
        tracing::info!("loaded css from {}", resolved.display());
    }
}

fn watch_for_config_changes(sender: ComponentSender<App>) {
    let config_dir = Config::config_directory();
    if !config_dir.exists() {
        tracing::error!("config directory {} does not exist", config_dir.display());
    }

    tracing::info!("watching config directory");

    relm4::spawn(async move {
        let mut debouncer = match new_debouncer(
            Duration::from_millis(200),
            None,
            move |res: DebounceEventResult| {
                let events = match res {
                    Ok(events) => events,
                    Err(_) => return,
                };

                let mut config_changed = false;
                let mut css_changed = false;

                for event in events {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                            for path in &event.paths {
                                if let Some(filename) = path.file_name() {
                                    match filename.to_str() {
                                        Some("config.toml") => config_changed = true,
                                        Some("theme.css") => css_changed = true,
                                        _ => {}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if config_changed {
                    tracing::debug!("config changed");
                    sender.input(Input::ConfigChanged(Config::load()));
                }
                if css_changed {
                    tracing::debug!("css changed");
                    sender.input(Input::CssChanged);
                }
            },
        ) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("failed to create watcher: {}", e);
                return;
            }
        };

        if let Err(e) = debouncer.watch(&config_dir, notify::RecursiveMode::NonRecursive) {
            tracing::error!("failed to watch config directory: {}", e);
            return;
        }

        for name in ["theme.css", "config.toml"] {
            let path = config_dir.join(name);
            if !path.is_symlink() {
                continue;
            }
            let Ok(resolved) = path.canonicalize() else {
                continue;
            };
            let Some(parent) = resolved.parent() else {
                continue;
            };
            if parent == config_dir {
                continue;
            }
            if let Err(e) = debouncer.watch(parent, notify::RecursiveMode::NonRecursive) {
                tracing::warn!("failed to watch symlink target for {}: {}", name, e);
            } else {
                tracing::info!("watching symlink target: {}", parent.display());
            }
        }

        std::future::pending::<()>().await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::config::{AppletConfig, Config, PanelConfig, PanelPosition};
    use toml::Value;

    fn panel(applets: &[&str]) -> PanelConfig {
        PanelConfig {
            position: PanelPosition::Top,
            height: 36,
            margin: Default::default(),
            applets: applets.iter().map(|name| name.to_string()).collect(),
        }
    }

    fn notifications_applet(settings: Value) -> AppletConfig {
        AppletConfig {
            extends: "notifications".to_string(),
            settings,
        }
    }

    #[test]
    fn popup_config_uses_first_notifications_applet_in_panel_order() {
        let mut config = Config::default();
        config.panels = vec![panel(&["clock", "notif-b", "notif-a"])];
        config.applets.insert(
            "notif-a".into(),
            notifications_applet(toml::from_str(r#"popup_position = "top-left""#).unwrap()),
        );
        config.applets.insert(
            "notif-b".into(),
            notifications_applet(toml::from_str(r#"popup_position = "bottom-right""#).unwrap()),
        );

        let popup = notifications_popup_config(&config).expect("popup config");
        assert_eq!(popup.popup_position, "bottom-right");
    }

    #[test]
    fn popup_config_returns_none_when_first_notifications_applet_disables_popup() {
        let mut config = Config::default();
        config.panels = vec![panel(&["notif-a", "notif-b"])];
        config.applets.insert(
            "notif-a".into(),
            notifications_applet(toml::from_str(r#"show_popup = false"#).unwrap()),
        );
        config.applets.insert(
            "notif-b".into(),
            notifications_applet(toml::from_str(r#"show_popup = true"#).unwrap()),
        );

        assert!(notifications_popup_config(&config).is_none());
    }
}
