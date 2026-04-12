use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
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

use glimpse::bluetooth::protocol::{
    BluetoothPrompt, BluetoothPromptKind, BluetoothPromptReply, BluetoothServiceCommand,
    BluetoothServiceState,
};
use glimpse::network::protocol::{
    NetworkPrompt, NetworkPromptId, NetworkPromptKind, NetworkPromptReply,
    NetworkServiceCommand as PanelNetworkServiceCommand, NetworkServiceState,
};

use crate::{
    applets::notifications::{
        NotificationActionCommand, NotificationPopup, NotificationPopupInit, NotificationsConfig,
    },
    backdrop,
    config::Config,
    panels,
    providers::dbus::DbusProvider,
    services::{Services, ServicesHandle},
    wallpaper,
};

pub struct App {
    config: Config,
    theme_css: CssProvider,
    panels: Vec<Controller<panels::Panel>>,
    wallpaper_windows: std::collections::HashMap<String, Controller<wallpaper::MonitorWindow>>,
    backdrop_windows: std::collections::HashMap<String, backdrop::BackdropWindow>,
    dbus: DbusProvider,
    services: Services,
    bluetooth_dialog: BluetoothPromptDialog,
    bluetooth_state: BluetoothServiceState,
    network_dialog: NetworkPromptDialog,
    network_state: NetworkServiceState,
    notification_popup: Option<Controller<NotificationPopup>>,
}

#[derive(Debug)]
pub enum Input {
    ConfigChanged(Config),
    CssChanged,
    MonitorsChanged,
    BluetoothState(BluetoothServiceState),
    BluetoothPromptReply {
        id: glimpse::bluetooth::protocol::BluetoothPromptId,
        reply: BluetoothPromptReply,
    },
    NetworkState(NetworkServiceState),
    NetworkPromptReply {
        id: NetworkPromptId,
        reply: NetworkPromptReply,
    },
    NotificationCommand(NotificationActionCommand),
}

struct BluetoothPromptDialog {
    parent: adw::ApplicationWindow,
    sender: ComponentSender<App>,
    dialog: Option<adw::AlertDialog>,
    current_prompt: Rc<RefCell<Option<BluetoothPrompt>>>,
}

struct NetworkPromptDialog {
    parent: adw::ApplicationWindow,
    dialog: NetworkPromptWidgets,
    current_prompt: Rc<RefCell<Option<NetworkPrompt>>>,
}

#[derive(Clone)]
struct NetworkPromptWidgets {
    dialog: adw::AlertDialog,
    entry: gtk::Entry,
    error_label: gtk::Label,
    cancel_button: gtk::Button,
    connect_button: gtk::Button,
    spinner: gtk::Spinner,
    submitting: Rc<Cell<bool>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BluetoothPromptMode {
    Display,
    Confirm,
    Pin,
    Passkey,
}

fn should_replace_prompt<T: PartialEq>(current_prompt: Option<&T>, next_prompt: &T) -> bool {
    current_prompt != Some(next_prompt)
}

fn clear_completed_prompt<T: PartialEq>(
    current_prompt: &Rc<RefCell<Option<T>>>,
    completed_prompt: &T,
) {
    let should_clear = current_prompt.borrow().as_ref() == Some(completed_prompt);
    if should_clear {
        *current_prompt.borrow_mut() = None;
    }
}

fn should_update_network_prompt_in_place(
    current_prompt: Option<&NetworkPrompt>,
    next_prompt: &NetworkPrompt,
) -> bool {
    let Some(current_prompt) = current_prompt else {
        return false;
    };

    match (&current_prompt.kind, &next_prompt.kind) {
        (
            NetworkPromptKind::WifiPassword { ssid: current_ssid },
            NetworkPromptKind::WifiPassword { ssid: next_ssid },
        ) => current_ssid == next_ssid,
    }
}

fn network_prompt_changed(
    current_prompt: Option<&NetworkPrompt>,
    next_prompt: Option<&NetworkPrompt>,
) -> bool {
    current_prompt != next_prompt
}

fn should_reset_network_prompt_form(
    current_prompt: Option<&NetworkPrompt>,
    next_prompt: Option<&NetworkPrompt>,
) -> bool {
    let Some(next_prompt) = next_prompt else {
        return true;
    };

    !should_update_network_prompt_in_place(current_prompt, next_prompt)
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
        let bluetooth_state = services.handle.bluetooth.subscribe().borrow().clone();
        let network_state = services.handle.network.subscribe().borrow().clone();
        let bluetooth_dialog = BluetoothPromptDialog::new(&root, sender.clone());
        let network_dialog = NetworkPromptDialog::new(&root, sender.clone());

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

        let (wallpaper_windows, backdrop_windows) = Display::default()
            .map(|d| {
                (
                    wallpaper::open_all_monitors(&d, &config.wallpaper),
                    backdrop::open_all_monitors(&d, &config.backdrop),
                )
            })
            .unwrap_or_default();

        let model = App {
            panels,
            wallpaper_windows,
            backdrop_windows,
            theme_css,
            config,
            dbus,
            services,
            bluetooth_dialog,
            bluetooth_state,
            network_dialog,
            network_state,
            notification_popup,
        };

        let bluetooth = model.services.handle.bluetooth.clone();
        let input = sender.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    let mut state_rx = bluetooth.subscribe();
                    input.input(Input::BluetoothState(state_rx.borrow().clone()));

                    loop {
                        if state_rx.changed().await.is_err() {
                            break;
                        }
                        input.input(Input::BluetoothState(state_rx.borrow().clone()));
                    }
                })
                .drop_on_shutdown()
        });

        let network = model.services.handle.network.clone();
        let input = sender.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    let mut state_rx = network.subscribe();
                    input.input(Input::NetworkState(state_rx.borrow().clone()));

                    loop {
                        if state_rx.changed().await.is_err() {
                            break;
                        }
                        input.input(Input::NetworkState(state_rx.borrow().clone()));
                    }
                })
                .drop_on_shutdown()
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
            Input::BluetoothState(state) => {
                self.bluetooth_state = state;
                self.bluetooth_dialog
                    .update(self.bluetooth_state.prompt.as_ref(), &self.bluetooth_state);
            }
            Input::BluetoothPromptReply { id, reply } => {
                let bluetooth = self.services.handle.bluetooth.clone();
                relm4::spawn(async move {
                    if let Err(error) = bluetooth
                        .send(BluetoothServiceCommand::PromptReply { id, reply })
                        .await
                    {
                        tracing::warn!(error = %error, "bluetooth app: failed to send prompt reply");
                    }
                });
            }
            Input::NetworkState(state) => {
                let prompt_changed = network_prompt_changed(
                    self.network_state.prompt.as_ref(),
                    state.prompt.as_ref(),
                );
                self.network_state = state;
                if prompt_changed {
                    self.network_dialog
                        .update(self.network_state.prompt.as_ref());
                }
            }
            Input::NetworkPromptReply { id, reply } => {
                let network = self.services.handle.network.clone();
                relm4::spawn(async move {
                    if let Err(error) = network
                        .send(PanelNetworkServiceCommand::PromptReply { id, reply })
                        .await
                    {
                        tracing::warn!(error = %error, "network app: failed to send prompt reply");
                    }
                });
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
    backdrop_windows: &mut std::collections::HashMap<String, backdrop::BackdropWindow>,
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
    backdrop_windows: &mut std::collections::HashMap<String, backdrop::BackdropWindow>,
) {
    for (_, window) in backdrop_windows.drain() {
        window.close();
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

impl BluetoothPromptDialog {
    fn new(root: &adw::ApplicationWindow, sender: ComponentSender<App>) -> Self {
        Self {
            parent: root.clone(),
            sender,
            dialog: None,
            current_prompt: Rc::new(RefCell::new(None)),
        }
    }

    fn update(&mut self, prompt: Option<&BluetoothPrompt>, state: &BluetoothServiceState) {
        let Some(prompt) = prompt.cloned() else {
            *self.current_prompt.borrow_mut() = None;
            if let Some(dialog) = self.dialog.take() {
                dialog.force_close();
            }
            return;
        };

        if !should_replace_prompt(self.current_prompt.borrow().as_ref(), &prompt) {
            return;
        }

        if let Some(dialog) = self.dialog.take() {
            dialog.force_close();
        }

        let (dialog, entry) = build_bluetooth_prompt_dialog(&prompt, state);
        let response_prompt = self.current_prompt.clone();
        let response_parent = self.parent.clone();
        let response_sender = self.sender.clone();
        let response_dialog = dialog.clone();
        let response_entry = entry.clone();

        *self.current_prompt.borrow_mut() = Some(prompt.clone());
        self.dialog = Some(dialog);

        relm4::spawn_local(async move {
            let response = response_dialog.choose_future(&response_parent).await;
            let active_prompt = response_prompt.borrow().clone();
            clear_completed_prompt(&response_prompt, &prompt);
            let Some(active_prompt) = active_prompt else {
                return;
            };

            if active_prompt.id != prompt.id {
                return;
            }

            let reply = bluetooth_prompt_reply(&active_prompt, response.as_str(), &response_entry);
            if let Some(reply) = reply {
                response_sender.input(Input::BluetoothPromptReply {
                    id: active_prompt.id,
                    reply,
                });
            }
        });
    }
}

impl NetworkPromptDialog {
    fn new(root: &adw::ApplicationWindow, sender: ComponentSender<App>) -> Self {
        let current_prompt = Rc::new(RefCell::new(None));
        let dialog = build_network_prompt_dialog(root, sender.clone(), current_prompt.clone());
        Self {
            parent: root.clone(),
            dialog,
            current_prompt,
        }
    }

    fn update(&mut self, prompt: Option<&NetworkPrompt>) {
        let reset_form =
            should_reset_network_prompt_form(self.current_prompt.borrow().as_ref(), prompt);

        let Some(prompt) = prompt.cloned() else {
            *self.current_prompt.borrow_mut() = None;
            clear_network_prompt_form(&self.dialog);
            self.dialog.dialog.force_close();
            return;
        };

        if reset_form {
            clear_network_prompt_form(&self.dialog);
        }

        *self.current_prompt.borrow_mut() = Some(prompt.clone());
        update_network_prompt_widgets(&self.dialog, &prompt);
        if reset_form {
            self.dialog.dialog.present(Some(&self.parent));
        }
    }
}

fn build_network_prompt_dialog(
    _parent: &adw::ApplicationWindow,
    sender: ComponentSender<App>,
    current_prompt: Rc<RefCell<Option<NetworkPrompt>>>,
) -> NetworkPromptWidgets {
    let dialog = adw::AlertDialog::new(Some("Wi-Fi Password"), Some(""));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let entry = gtk::Entry::new();
    entry.set_visibility(false);
    entry.set_activates_default(false);
    entry.set_placeholder_text(Some("Password"));
    entry.set_input_purpose(gtk::InputPurpose::Password);
    let error_label = gtk::Label::new(None);
    error_label.set_halign(gtk::Align::Start);
    error_label.set_xalign(0.0);
    error_label.set_wrap(true);
    error_label.add_css_class("error");
    error_label.set_visible(false);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let cancel_button = gtk::Button::with_label("Cancel");
    actions.append(&cancel_button);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    actions.append(&spacer);
    let spinner = gtk::Spinner::new();
    spinner.set_visible(false);

    let connect_button = gtk::Button::with_label("Connect");
    connect_button.add_css_class("suggested-action");
    let submitting = Rc::new(Cell::new(false));
    let submitting_state = submitting.clone();
    let connect_button_for_change = connect_button.clone();
    entry.connect_changed(move |entry| {
        connect_button_for_change
            .set_sensitive(!submitting_state.get() && !entry.text().trim().is_empty());
    });
    entry.grab_focus();
    content.append(&entry);
    content.append(&error_label);
    actions.append(&spinner);
    actions.append(&connect_button);
    content.append(&actions);
    dialog.set_extra_child(Some(&content));

    let widgets = NetworkPromptWidgets {
        dialog,
        entry,
        error_label,
        cancel_button,
        connect_button,
        spinner,
        submitting,
    };
    {
        let response_prompt = current_prompt.clone();
        let response_sender = sender.clone();
        let response_entry = widgets.entry.clone();
        widgets.connect_button.connect_clicked(move |_| {
            let Some(active_prompt) = response_prompt.borrow().clone() else {
                return;
            };
            if let Some(reply) = network_submit_prompt_reply(&response_entry) {
                response_sender.input(Input::NetworkPromptReply {
                    id: active_prompt.id,
                    reply,
                });
            }
        });
    }
    {
        let response_button = widgets.connect_button.clone();
        widgets.entry.connect_activate(move |_| {
            response_button.emit_clicked();
        });
    }
    {
        let response_prompt = current_prompt.clone();
        let response_sender = sender.clone();
        let response_dialog = widgets.dialog.clone();
        widgets.cancel_button.connect_clicked(move |_| {
            let Some(active_prompt) = response_prompt.borrow().clone() else {
                return;
            };
            if active_prompt.submitting {
                return;
            }
            *response_prompt.borrow_mut() = None;
            response_dialog.force_close();
            response_sender.input(Input::NetworkPromptReply {
                id: active_prompt.id,
                reply: NetworkPromptReply::Cancel,
            });
        });
    }
    {
        let response_prompt = current_prompt.clone();
        let response_sender = sender.clone();
        widgets.dialog.connect_closed(move |_| {
            let Some(active_prompt) = response_prompt.borrow().clone() else {
                return;
            };
            if active_prompt.submitting {
                return;
            }
            *response_prompt.borrow_mut() = None;
            response_sender.input(Input::NetworkPromptReply {
                id: active_prompt.id,
                reply: NetworkPromptReply::Cancel,
            });
        });
    }
    widgets
}

fn update_network_prompt_widgets(widgets: &NetworkPromptWidgets, prompt: &NetworkPrompt) {
    let was_submitting = widgets.submitting.get();
    let body = match &prompt.kind {
        NetworkPromptKind::WifiPassword { ssid } => format!("Enter the password for {ssid}."),
    };
    widgets.dialog.set_heading(Some("Wi-Fi Password"));
    widgets.dialog.set_body(&body);
    widgets.submitting.set(prompt.submitting);
    widgets.dialog.set_can_close(!prompt.submitting);
    widgets.entry.set_sensitive(!prompt.submitting);
    widgets.cancel_button.set_sensitive(!prompt.submitting);
    widgets.spinner.set_visible(prompt.submitting);
    if prompt.submitting {
        widgets.spinner.start();
    } else {
        widgets.spinner.stop();
    }
    let error_text = prompt.error_message.as_deref().unwrap_or_default();
    widgets.error_label.set_label(error_text);
    widgets.error_label.set_visible(!error_text.is_empty());
    widgets
        .connect_button
        .set_sensitive(!prompt.submitting && !widgets.entry.text().trim().is_empty());
    if was_submitting && !prompt.submitting {
        widgets.entry.grab_focus();
    }
}

fn clear_network_prompt_form(widgets: &NetworkPromptWidgets) {
    widgets.entry.set_text("");
    widgets.entry.set_position(-1);
}

fn network_submit_prompt_reply(entry: &gtk::Entry) -> Option<NetworkPromptReply> {
    let value = entry.text().trim().to_string();
    if value.is_empty() {
        tracing::warn!("network dialog: empty password submitted");
        None
    } else {
        Some(NetworkPromptReply::SubmitPassword(value))
    }
}

fn bluetooth_dialog_content(
    prompt: &BluetoothPrompt,
    state: &BluetoothServiceState,
) -> (String, String, Option<String>, BluetoothPromptMode) {
    let label = bluetooth_prompt_device_label(prompt, state);
    match &prompt.kind {
        BluetoothPromptKind::Confirm { passkey } => (
            "Confirm Pairing".into(),
            format!("Does the code on {label} match this one?"),
            Some(format!("{:06}", passkey)),
            BluetoothPromptMode::Confirm,
        ),
        BluetoothPromptKind::RequestPin => (
            "Enter PIN".into(),
            format!("Enter the PIN shown by {label}."),
            None,
            BluetoothPromptMode::Pin,
        ),
        BluetoothPromptKind::RequestPasskey => (
            "Enter Passkey".into(),
            format!("Enter the passkey shown by {label}."),
            None,
            BluetoothPromptMode::Passkey,
        ),
        BluetoothPromptKind::DisplayPin { pincode } => (
            "Bluetooth Pairing".into(),
            format!("Type this PIN on {label} and press Enter."),
            Some(pincode.clone()),
            BluetoothPromptMode::Display,
        ),
        BluetoothPromptKind::DisplayPasskey { passkey, entered } => {
            let progress = if *entered > 0 {
                format!(" Typed on device: {entered}.")
            } else {
                String::new()
            };
            (
                "Bluetooth Pairing".into(),
                format!("Type this passkey on {label} and press Enter.{progress}"),
                Some(format!("{:06}", passkey)),
                BluetoothPromptMode::Display,
            )
        }
    }
}

fn bluetooth_prompt_device_label(
    prompt: &BluetoothPrompt,
    state: &BluetoothServiceState,
) -> String {
    if !prompt.device_label.is_empty() && prompt.device_label != prompt.device_path {
        return prompt.device_label.clone();
    }

    if let Some(address) = bluetooth_prompt_address(&prompt.device_path) {
        if let Some(device) = state
            .snapshot
            .devices
            .iter()
            .find(|device| device.address == address)
        {
            return device.name.clone();
        }
    }

    prompt.device_path.clone()
}

fn bluetooth_prompt_address(path: &str) -> Option<String> {
    let tail = path.rsplit('/').next()?;
    let suffix = tail.strip_prefix("dev_")?;
    Some(suffix.replace('_', ":"))
}

fn build_bluetooth_prompt_dialog(
    prompt: &BluetoothPrompt,
    state: &BluetoothServiceState,
) -> (adw::AlertDialog, gtk::Entry) {
    const RESPONSE_CANCEL: &str = "cancel";
    const RESPONSE_ACCEPT: &str = "accept";

    let (heading, body, code, mode) = bluetooth_dialog_content(prompt, state);
    let dialog = adw::AlertDialog::new(Some(&heading), Some(&body));
    dialog.add_response(RESPONSE_CANCEL, "Cancel");
    dialog.set_close_response(RESPONSE_CANCEL);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let code_label = gtk::Label::new(code.as_deref());
    code_label.set_visible(code.is_some());
    code_label.set_selectable(true);
    code_label.set_xalign(0.0);
    code_label.set_halign(gtk::Align::Start);
    content.append(&code_label);

    let entry = gtk::Entry::new();
    entry.set_visible(false);
    content.append(&entry);

    match mode {
        BluetoothPromptMode::Display => {}
        BluetoothPromptMode::Confirm => {
            dialog.add_response(RESPONSE_ACCEPT, "Pair");
            dialog.set_default_response(Some(RESPONSE_ACCEPT));
            dialog.set_response_appearance(RESPONSE_ACCEPT, adw::ResponseAppearance::Suggested);
        }
        BluetoothPromptMode::Pin => {
            dialog.add_response(RESPONSE_ACCEPT, "Submit PIN");
            dialog.set_default_response(Some(RESPONSE_ACCEPT));
            dialog.set_response_appearance(RESPONSE_ACCEPT, adw::ResponseAppearance::Suggested);
            dialog.set_response_enabled(RESPONSE_ACCEPT, false);
            entry.set_visible(true);
            entry.set_placeholder_text(Some("PIN"));
            entry.set_input_purpose(gtk::InputPurpose::Digits);
            let validation_dialog = dialog.clone();
            entry.connect_changed(move |entry| {
                let valid = !entry.text().trim().is_empty();
                validation_dialog.set_response_enabled(RESPONSE_ACCEPT, valid);
            });
            entry.grab_focus();
        }
        BluetoothPromptMode::Passkey => {
            dialog.add_response(RESPONSE_ACCEPT, "Submit Passkey");
            dialog.set_default_response(Some(RESPONSE_ACCEPT));
            dialog.set_response_appearance(RESPONSE_ACCEPT, adw::ResponseAppearance::Suggested);
            dialog.set_response_enabled(RESPONSE_ACCEPT, false);
            entry.set_visible(true);
            entry.set_placeholder_text(Some("Passkey"));
            entry.set_input_purpose(gtk::InputPurpose::Digits);
            let validation_dialog = dialog.clone();
            entry.connect_changed(move |entry| {
                let valid = entry.text().trim().parse::<u32>().is_ok();
                validation_dialog.set_response_enabled(RESPONSE_ACCEPT, valid);
            });
            entry.grab_focus();
        }
    }

    if code.is_some() || mode == BluetoothPromptMode::Pin || mode == BluetoothPromptMode::Passkey {
        dialog.set_extra_child(Some(&content));
    }

    (dialog, entry)
}

fn bluetooth_prompt_reply(
    prompt: &BluetoothPrompt,
    response: &str,
    entry: &gtk::Entry,
) -> Option<BluetoothPromptReply> {
    match response {
        "cancel" => Some(BluetoothPromptReply::Cancel),
        "accept" => match &prompt.kind {
            BluetoothPromptKind::Confirm { .. } => Some(BluetoothPromptReply::Confirm),
            BluetoothPromptKind::RequestPin => {
                let value = entry.text().trim().to_owned();
                if value.is_empty() {
                    tracing::warn!("bluetooth dialog: empty pin submitted");
                    None
                } else {
                    Some(BluetoothPromptReply::Pin(value))
                }
            }
            BluetoothPromptKind::RequestPasskey => {
                let value = entry.text();
                match value.trim().parse::<u32>() {
                    Ok(passkey) => Some(BluetoothPromptReply::Passkey(passkey)),
                    Err(error) => {
                        tracing::warn!(error = %error, value = %value, "bluetooth dialog: invalid passkey submitted");
                        None
                    }
                }
            }
            BluetoothPromptKind::DisplayPin { .. } | BluetoothPromptKind::DisplayPasskey { .. } => {
                None
            }
        },
        _ => None,
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
    use crate::config::{AppletConfig, Config, PanelConfig, PanelPosition};
    use toml::Value;

    fn network_prompt(id: u64, ssid: &str) -> NetworkPrompt {
        NetworkPrompt {
            id: NetworkPromptId(id),
            kind: NetworkPromptKind::WifiPassword { ssid: ssid.into() },
            error_message: None,
            submitting: false,
        }
    }

    fn bluetooth_prompt(id: u64) -> BluetoothPrompt {
        BluetoothPrompt {
            id: glimpse::bluetooth::protocol::BluetoothPromptId(id),
            device_path: "/org/bluez/hci0/dev_AA_BB".into(),
            device_label: "Headphones".into(),
            kind: BluetoothPromptKind::RequestPin,
        }
    }

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
    fn completed_network_prompt_is_cleared_for_replacement() {
        let prompt = network_prompt(1, "Skylink");
        let current = Rc::new(RefCell::new(Some(prompt.clone())));

        clear_completed_prompt(&current, &prompt);

        assert!(should_replace_prompt(current.borrow().as_ref(), &prompt));
    }

    #[test]
    fn completed_bluetooth_prompt_is_cleared_for_replacement() {
        let prompt = bluetooth_prompt(7);
        let current = Rc::new(RefCell::new(Some(prompt.clone())));

        clear_completed_prompt(&current, &prompt);

        assert!(should_replace_prompt(current.borrow().as_ref(), &prompt));
    }

    #[test]
    fn wifi_password_form_state_is_preserved_for_same_ssid() {
        let current = network_prompt(1, "Skylink");
        let next = NetworkPrompt {
            id: NetworkPromptId(2),
            kind: NetworkPromptKind::WifiPassword {
                ssid: "Skylink".into(),
            },
            error_message: Some("Incorrect password. Try again.".into()),
            submitting: false,
        };

        assert!(!should_reset_network_prompt_form(
            Some(&current),
            Some(&next)
        ));
    }

    #[test]
    fn wifi_password_prompt_updates_in_place_for_same_ssid_even_with_new_id() {
        let current = network_prompt(1, "Skylink");
        let next = NetworkPrompt {
            id: NetworkPromptId(2),
            kind: NetworkPromptKind::WifiPassword {
                ssid: "Skylink".into(),
            },
            error_message: Some("Incorrect password. Try again.".into()),
            submitting: false,
        };

        assert!(should_update_network_prompt_in_place(Some(&current), &next));
    }

    #[test]
    fn wifi_password_prompt_rebuilds_for_different_ssid() {
        let current = network_prompt(1, "Skylink");
        let next = network_prompt(2, "Office");

        assert!(!should_update_network_prompt_in_place(
            Some(&current),
            &next
        ));
    }

    #[test]
    fn wifi_password_form_state_is_reset_for_new_prompt() {
        let next = network_prompt(1, "Skylink");

        assert!(should_reset_network_prompt_form(None, Some(&next)));
    }

    #[test]
    fn unchanged_network_prompt_does_not_require_dialog_update() {
        let prompt = network_prompt(1, "Skylink");

        assert!(!network_prompt_changed(Some(&prompt), Some(&prompt)));
    }

    #[test]
    fn changed_network_prompt_requires_dialog_update() {
        let current = network_prompt(1, "Skylink");
        let next = NetworkPrompt {
            id: NetworkPromptId(1),
            kind: NetworkPromptKind::WifiPassword {
                ssid: "Skylink".into(),
            },
            error_message: Some("Incorrect password. Try again.".into()),
            submitting: false,
        };

        assert!(network_prompt_changed(Some(&current), Some(&next)));
    }

    #[test]
    fn wifi_password_form_state_is_reset_for_different_ssid() {
        let current = network_prompt(1, "Skylink");
        let next = network_prompt(2, "Office");

        assert!(should_reset_network_prompt_form(
            Some(&current),
            Some(&next)
        ));
    }

    #[test]
    fn wifi_password_form_state_is_reset_when_prompt_closes() {
        let current = network_prompt(1, "Skylink");

        assert!(should_reset_network_prompt_form(Some(&current), None));
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
