use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::Arc, time::Duration};

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
use glimpse_client::Client;

use crate::{
    config::Config,
    panels,
    providers::dbus::DbusProvider,
    services::{Services, ServicesHandle},
};

pub struct App {
    config: Config,
    theme_css: CssProvider,
    panels: Vec<Controller<panels::Panel>>,
    dbus: DbusProvider,
    client: Option<Arc<Client>>,
    services: Services,
    bluetooth_dialog: BluetoothPromptDialog,
    bluetooth_state: BluetoothServiceState,
    network_dialog: NetworkPromptDialog,
    network_state: NetworkServiceState,
}

#[derive(Debug)]
pub enum Input {
    ConfigChanged(Config),
    CssChanged,
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
}

struct BluetoothPromptDialog {
    parent: adw::ApplicationWindow,
    sender: ComponentSender<App>,
    dialog: Option<adw::AlertDialog>,
    current_prompt: Rc<RefCell<Option<BluetoothPrompt>>>,
}

struct NetworkPromptDialog {
    parent: adw::ApplicationWindow,
    sender: ComponentSender<App>,
    dialog: Option<adw::AlertDialog>,
    current_prompt: Rc<RefCell<Option<NetworkPrompt>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BluetoothPromptMode {
    Display,
    Confirm,
    Pin,
    Passkey,
}

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_ACCEPT: &str = "accept";

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

        let dbus = DbusProvider::connect();
        let services = Services::new(dbus.system.clone());
        let bluetooth_state = services.handle.bluetooth.subscribe().borrow().clone();
        let network_state = services.handle.network.subscribe().borrow().clone();
        let bluetooth_dialog = BluetoothPromptDialog::new(&root, sender.clone());
        let network_dialog = NetworkPromptDialog::new(&root, sender.clone());

        let client = match tokio::runtime::Handle::current().block_on(Client::connect()) {
            Ok(c) => Some(Arc::new(c)),
            Err(e) => {
                tracing::warn!("glimpsed not available: {e}");
                None
            }
        };

        let panels = setup_panels(
            &config,
            dbus.session.clone(),
            dbus.system.clone(),
            client.clone(),
            services.handle.clone(),
        );

        let model = App {
            panels,
            theme_css,
            config,
            dbus,
            client,
            services,
            bluetooth_dialog,
            bluetooth_state,
            network_dialog,
            network_state,
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
                self.panels = setup_panels(
                    &new_config,
                    self.dbus.session.clone(),
                    self.dbus.system.clone(),
                    self.client.clone(),
                    self.services.handle.clone(),
                );
                self.config = new_config;
            }
            Input::CssChanged => {
                load_css(&self.theme_css, &self.config.theme_path());
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
                self.network_state = state;
                self.network_dialog
                    .update(self.network_state.prompt.as_ref());
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
        }
    }
}

fn setup_panels(
    config: &Config,
    dbus: zbus::Connection,
    system: zbus::Connection,
    client: Option<Arc<Client>>,
    services: ServicesHandle,
) -> Vec<Controller<panels::Panel>> {
    let mut panels = vec![];
    for panel_config in &config.panels {
        let panel_init = panels::Init {
            config: panel_config.clone(),
            applet_configs: config.applets.clone(),
            dbus: dbus.clone(),
            system: system.clone(),
            client: client.clone(),
            services: services.clone(),
        };
        let panel = panels::Panel::builder().launch(panel_init).detach();
        panels.push(panel);
    }
    panels
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
        Self {
            parent: root.clone(),
            sender,
            dialog: None,
            current_prompt: Rc::new(RefCell::new(None)),
        }
    }

    fn update(&mut self, prompt: Option<&NetworkPrompt>) {
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

        let (dialog, entry) = build_network_prompt_dialog(&prompt);
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
            if let Some(reply) = network_prompt_reply(response.as_str(), &response_entry) {
                response_sender.input(Input::NetworkPromptReply {
                    id: active_prompt.id,
                    reply,
                });
            }
        });
    }
}

fn build_network_prompt_dialog(prompt: &NetworkPrompt) -> (adw::AlertDialog, gtk::Entry) {
    let ssid = match &prompt.kind {
        NetworkPromptKind::WifiPassword { ssid } => ssid,
    };

    let dialog = adw::AlertDialog::new(
        Some("Wi-Fi Password"),
        Some(&format!("Enter the password for {ssid}.")),
    );
    dialog.add_response(RESPONSE_CANCEL, "Cancel");
    dialog.add_response(RESPONSE_ACCEPT, "Connect");
    dialog.set_close_response(RESPONSE_CANCEL);
    dialog.set_default_response(Some(RESPONSE_ACCEPT));
    dialog.set_response_appearance(RESPONSE_ACCEPT, adw::ResponseAppearance::Suggested);
    dialog.set_response_enabled(RESPONSE_ACCEPT, false);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let entry = gtk::Entry::new();
    entry.set_visibility(false);
    entry.set_activates_default(true);
    entry.set_placeholder_text(Some("Password"));
    entry.set_input_purpose(gtk::InputPurpose::Password);
    let validation_dialog = dialog.clone();
    entry.connect_changed(move |entry| {
        validation_dialog.set_response_enabled(RESPONSE_ACCEPT, !entry.text().trim().is_empty());
    });
    entry.grab_focus();
    content.append(&entry);
    dialog.set_extra_child(Some(&content));

    (dialog, entry)
}

fn network_prompt_reply(response: &str, entry: &gtk::Entry) -> Option<NetworkPromptReply> {
    match response {
        RESPONSE_CANCEL => Some(NetworkPromptReply::Cancel),
        RESPONSE_ACCEPT => {
            let value = entry.text().trim().to_string();
            if value.is_empty() {
                tracing::warn!("network dialog: empty password submitted");
                None
            } else {
                Some(NetworkPromptReply::SubmitPassword(value))
            }
        }
        _ => None,
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

    fn network_prompt(id: u64, ssid: &str) -> NetworkPrompt {
        NetworkPrompt {
            id: NetworkPromptId(id),
            kind: NetworkPromptKind::WifiPassword { ssid: ssid.into() },
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
}
