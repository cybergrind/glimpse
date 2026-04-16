use std::collections::HashMap;

use adw::prelude::*;
use gtk4_layer_shell::LayerShell;
use relm4::{
    ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
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
mod theme_runtime;
mod watchers;

use background_manager::sync_background_windows;
use notification_popup_manager::{setup_notification_popup, sync_notification_popup};
use panel_manager::{PanelState, reconfigure_panels, setup_panels};
use theme_runtime::{
    apply_theme_mode, sync_accent_css, sync_base_css, sync_structure_css, sync_theme_css,
};
use watchers::watch_for_config_changes;

pub struct App {
    window: adw::ApplicationWindow,
    config: Config,
    base_css: CssProvider,
    structure_css: CssProvider,
    accent_css: CssProvider,
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

        let base_css = CssProvider::new();
        sync_base_css(&base_css);
        let structure_css = CssProvider::new();
        sync_structure_css(&structure_css);
        let accent_css = CssProvider::new();
        sync_accent_css(&accent_css);
        let theme_css = CssProvider::new();
        sync_theme_css(&theme_css, &config);
        if let Some(display) = Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &base_css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            gtk::style_context_add_provider_for_display(
                &display,
                &structure_css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            gtk::style_context_add_provider_for_display(
                &display,
                &accent_css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
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
        let services = Services::new(
            dbus.session.clone(),
            dbus.system.clone(),
            config.night_light.clone(),
        );

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
        apply_theme_mode_to_windows(&root, &notification_popup, config.theme.mode);

        let model = App {
            window: root.clone(),
            panels,
            wallpaper_windows: HashMap::new(),
            backdrop_windows: HashMap::new(),
            base_css,
            structure_css,
            accent_css,
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
                if self.config.night_light != new_config.night_light {
                    let night_light = self.services.handle.night_light.clone();
                    let config = new_config.night_light.clone();
                    relm4::spawn(async move {
                        if let Err(error) = night_light
                            .send(glimpse::night_light::NightLightCommand::ApplyConfig(config))
                            .await
                        {
                            tracing::warn!(error = %error, "night light service: failed to send config update");
                        }
                    });
                }
                sync_base_css(&self.base_css);
                sync_structure_css(&self.structure_css);
                sync_accent_css(&self.accent_css);
                sync_theme_css(&self.theme_css, &new_config);
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
                apply_theme_mode_to_windows(
                    &self.window,
                    &self.notification_popup,
                    new_config.theme.mode,
                );
                self.config = new_config;
            }
            Input::CssChanged => {
                sync_base_css(&self.base_css);
                sync_structure_css(&self.structure_css);
                sync_accent_css(&self.accent_css);
                sync_theme_css(&self.theme_css, &self.config);
                apply_theme_mode_to_windows(
                    &self.window,
                    &self.notification_popup,
                    self.config.theme.mode,
                );
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

fn apply_theme_mode_to_windows(
    window: &adw::ApplicationWindow,
    notification_popup: &Option<Controller<NotificationPopup>>,
    mode: glimpse::config::ThemeMode,
) {
    apply_theme_mode(window, mode);

    if let Some(notification_popup) = notification_popup {
        apply_theme_mode(notification_popup.widget(), mode);
    }
}
