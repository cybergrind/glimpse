use std::collections::HashMap;

use adw::prelude::*;
use gtk4_layer_shell::LayerShell;
use relm4::{
    ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, CssProvider, gdk::Display},
};

use glimpse::backdrop;
use glimpse::config::Config;
use glimpse::wallpaper;

use crate::{
    applets::notifications::{NotificationActionCommand, NotificationPopup},
    providers::dbus::DbusProvider,
    services::Services,
};

mod background_manager;
mod notification_popup_manager;
mod panel_manager;
mod watchers;

use background_manager::sync_background_windows;
use notification_popup_manager::{setup_notification_popup, sync_notification_popup};
use panel_manager::{PanelState, reconfigure_panels, setup_panels};
use watchers::{load_css, watch_for_config_changes};

pub struct App {
    config: Config,
    theme_css: CssProvider,
    panels: Vec<PanelState>,
    wallpaper_windows: HashMap<String, Controller<wallpaper::MonitorWindow>>,
    backdrop_windows: HashMap<String, Controller<backdrop::BackdropWindow>>,
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
            wallpaper_windows: HashMap::new(),
            backdrop_windows: HashMap::new(),
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

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Input::ConfigChanged(new_config) => {
                reconfigure_panels(
                    &mut self.panels,
                    &new_config,
                    self.dbus.session.clone(),
                    self.dbus.system.clone(),
                    self.services.handle.clone(),
                );
                if self.config.wallpaper != new_config.wallpaper
                    || self.config.backdrop != new_config.backdrop
                {
                    sync_background_windows(
                        Display::default(),
                        &new_config,
                        &mut self.wallpaper_windows,
                        &mut self.backdrop_windows,
                    );
                }
                sync_notification_popup(
                    &self.config,
                    &new_config,
                    &mut self.notification_popup,
                    self.services.handle.notifications.clone(),
                    sender,
                );
                self.config = new_config;
            }
            Input::CssChanged => {
                load_css(&self.theme_css, &self.config.theme_path());
            }
            Input::MonitorsChanged => {
                sync_background_windows(
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
