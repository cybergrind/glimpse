use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self},
};
use tokio_util::sync::CancellationToken;

use crate::agents::bluetooth::{
    BluetoothAgentHandle, BluetoothPrompt, BluetoothPromptId, BluetoothPromptKind,
    BluetoothPromptReply,
};
use crate::theme;
use glimpse_core::{ThemeMode, services::bluetooth::BluetoothSnapshot};

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_ACCEPT: &str = "accept";
const MAX_PASSKEY: u32 = 999_999;

pub struct PromptHost {
    agent: BluetoothAgentHandle,
    dialog: Controller<PromptDialog>,
    subscription_cancel: CancellationToken,
}

pub struct PromptHostInit {
    pub agent: BluetoothAgentHandle,
    pub parent: gtk::Widget,
    pub theme_mode: ThemeMode,
}

#[derive(Debug)]
pub enum PromptHostInput {
    SetParent(gtk::Widget),
    SetThemeMode(ThemeMode),
    DialogOutput(PromptDialogOutput),
}

#[relm4::component(pub)]
impl Component for PromptHost {
    type Init = PromptHostInit;
    type Input = PromptHostInput;
    type Output = ();
    type CommandOutput = Option<BluetoothPrompt>;

    view! {
        gtk::Box {
            set_visible: false,
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let dialog = PromptDialog::builder()
            .launch(PromptDialogInit {
                parent: init.parent,
                theme_mode: init.theme_mode,
            })
            .forward(sender.input_sender(), PromptHostInput::DialogOutput);

        let model = PromptHost {
            agent: init.agent,
            dialog,
            subscription_cancel: CancellationToken::new(),
        };

        let agent = model.agent.clone();
        let cancel = model.subscription_cancel.clone();
        let command_sender = sender.command_sender().clone();
        relm4::spawn(async move {
            let mut sub = agent.subscribe();
            let _ = command_sender.send(sub.borrow().clone());

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() {
                            break;
                        }

                        let _ = command_sender.send(sub.borrow().clone());
                    }
                }
            }
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            PromptHostInput::SetParent(parent) => {
                self.dialog.emit(PromptDialogInput::SetParent(parent));
            }
            PromptHostInput::SetThemeMode(mode) => {
                self.dialog.emit(PromptDialogInput::SetThemeMode(mode));
            }
            PromptHostInput::DialogOutput(PromptDialogOutput::Reply { id, reply }) => {
                self.send_reply(id, reply);
            }
        }
    }

    fn update_cmd(
        &mut self,
        state: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        self.dialog.emit(PromptDialogInput::Update {
            prompt: state,
            snapshot: BluetoothSnapshot::default(),
        });
    }
}

impl PromptHost {
    fn send_reply(&self, id: BluetoothPromptId, reply: BluetoothPromptReply) {
        if !self.agent.reply(id, reply) {
            tracing::warn!(prompt_id = id.0, "failed to send bluetooth prompt reply");
        }
    }
}

impl Drop for PromptHost {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

pub struct PromptDialogInit {
    pub parent: gtk::Widget,
    pub theme_mode: ThemeMode,
}

pub struct PromptDialog {
    parent: gtk::Widget,
    root: gtk::Box,
    dialog: adw::AlertDialog,
    current_prompt: Rc<RefCell<Option<BluetoothPrompt>>>,
    dismissed_display_prompt: Rc<Cell<Option<BluetoothPromptId>>>,
    generation: Rc<Cell<u64>>,
    code: String,
    entry_text: String,
    mode: PromptMode,
    theme_mode: ThemeMode,
    entry: gtk::Entry,
}

#[derive(Debug, Clone)]
pub enum PromptDialogInput {
    Update {
        prompt: Option<BluetoothPrompt>,
        snapshot: BluetoothSnapshot,
    },
    SetParent(gtk::Widget),
    SetThemeMode(ThemeMode),
    EntryChanged(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptDialogOutput {
    Reply {
        id: BluetoothPromptId,
        reply: BluetoothPromptReply,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PromptMode {
    #[default]
    Display,
    Confirm,
    Pin,
    Passkey,
}

#[relm4::component(pub)]
impl SimpleComponent for PromptDialog {
    type Init = PromptDialogInit;
    type Input = PromptDialogInput;
    type Output = PromptDialogOutput;

    view! {
        gtk::Box {
            add_css_class: "bluetooth-prompt",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,

            gtk::Label {
                add_css_class: "bluetooth-prompt__code",
                #[watch]
                set_visible: model.shows_code(),
                #[watch]
                set_label: &model.code,
                set_xalign: 0.0,
                set_halign: gtk::Align::Start,
                set_wrap: true,
                set_selectable: true,
            },

            #[name(entry)]
            gtk::Entry {
                add_css_class: "bluetooth-prompt__entry",
                #[watch]
                set_visible: model.shows_entry(),
                #[watch]
                set_placeholder_text: Some(model.entry_placeholder()),
                set_input_purpose: gtk::InputPurpose::Digits,
                connect_changed[sender] => move |entry| {
                    sender.input(PromptDialogInput::EntryChanged(entry.text().to_string()));
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let dialog = adw::AlertDialog::new(None, None);
        dialog.add_response(RESPONSE_CANCEL, "Cancel");
        dialog.set_close_response(RESPONSE_CANCEL);
        theme::apply_theme_mode(&dialog, &theme::DIALOG_THEME_MODE);
        theme::apply_theme_mode(&root, &theme::DIALOG_THEME_MODE);
        dialog.set_extra_child(Some(&root));

        let model = PromptDialog {
            parent: init.parent,
            root: root.clone(),
            dialog,
            current_prompt: Rc::new(RefCell::new(None)),
            dismissed_display_prompt: Rc::new(Cell::new(None)),
            generation: Rc::new(Cell::new(0)),
            code: String::new(),
            entry_text: String::new(),
            mode: PromptMode::Display,
            theme_mode: init.theme_mode,
            entry: gtk::Entry::new(),
        };

        let widgets = view_output!();
        let mut model = model;
        model.entry = widgets.entry.clone();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PromptDialogInput::Update { prompt, snapshot } => {
                let Some(prompt) = prompt else {
                    self.entry_text.clear();
                    *self.current_prompt.borrow_mut() = None;
                    self.dismissed_display_prompt.set(None);
                    self.generation.set(self.generation.get().wrapping_add(1));
                    self.dialog.force_close();
                    return;
                };

                if self.dismissed_display_prompt.get() == Some(prompt.id)
                    && is_display_prompt(&prompt.kind)
                {
                    return;
                }

                let state = PromptViewState::from_prompt(&prompt, &snapshot);
                if self
                    .current_prompt
                    .borrow()
                    .as_ref()
                    .is_some_and(|current| current.id == prompt.id)
                {
                    self.apply_view_state(&state);
                    *self.current_prompt.borrow_mut() = Some(prompt);
                    return;
                }

                self.dialog.force_close();

                self.entry_text.clear();
                self.dismissed_display_prompt.set(None);
                self.apply_view_state(&state);

                if !self.entry.text().is_empty() {
                    self.entry.set_text("");
                }

                *self.current_prompt.borrow_mut() = Some(prompt.clone());
                let generation = self.generation.get().wrapping_add(1);
                self.generation.set(generation);

                let response_parent = self.parent.clone();
                let response_sender = sender.clone();
                let response_dialog = self.dialog.clone();
                let response_entry = self.entry.clone();
                let response_prompt = self.current_prompt.clone();
                let response_dismissed_display_prompt = self.dismissed_display_prompt.clone();
                let response_generation = self.generation.clone();
                let expected_prompt = prompt;

                relm4::spawn_local(async move {
                    let response = response_dialog.choose_future(Some(&response_parent)).await;
                    let active_prompt = response_prompt.borrow().clone();

                    let Some(active_prompt) = active_prompt else {
                        return;
                    };

                    if active_prompt.id != expected_prompt.id {
                        return;
                    }

                    if response_generation.get() != generation {
                        return;
                    }

                    *response_prompt.borrow_mut() = None;

                    if is_display_prompt(&active_prompt.kind) {
                        response_dismissed_display_prompt.set(Some(active_prompt.id));
                        return;
                    }

                    let reply = prompt_reply_text(
                        &active_prompt,
                        response.as_str(),
                        response_entry.text().as_str(),
                    );
                    if let Some(reply) = reply {
                        let _ = response_sender.output(PromptDialogOutput::Reply {
                            id: active_prompt.id,
                            reply,
                        });
                    }
                });

                if self.shows_entry() {
                    self.entry.grab_focus();
                }
            }
            PromptDialogInput::SetParent(parent) => {
                self.parent = parent;
            }
            PromptDialogInput::SetThemeMode(mode) => {
                self.theme_mode = mode;
                theme::apply_theme_mode(&self.dialog, &theme::DIALOG_THEME_MODE);
                theme::apply_theme_mode(&self.root, &theme::DIALOG_THEME_MODE);
            }
            PromptDialogInput::EntryChanged(text) => {
                self.entry_text = text;
                if self.dialog.has_response(RESPONSE_ACCEPT) {
                    self.dialog
                        .set_response_enabled(RESPONSE_ACCEPT, self.accept_enabled());
                }
            }
        }
    }
}

impl PromptDialog {
    fn shows_code(&self) -> bool {
        !self.code.is_empty()
    }

    fn shows_entry(&self) -> bool {
        matches!(self.mode, PromptMode::Pin | PromptMode::Passkey)
    }

    fn shows_accept(&self) -> bool {
        !matches!(self.mode, PromptMode::Display)
    }

    fn entry_placeholder(&self) -> &'static str {
        match self.mode {
            PromptMode::Pin => "PIN",
            PromptMode::Passkey => "Passkey",
            PromptMode::Display | PromptMode::Confirm => "",
        }
    }

    fn accept_label(&self) -> &'static str {
        match self.mode {
            PromptMode::Confirm => "Pair",
            PromptMode::Pin => "Submit PIN",
            PromptMode::Passkey => "Submit Passkey",
            PromptMode::Display => "",
        }
    }

    fn accept_enabled(&self) -> bool {
        match self.mode {
            PromptMode::Display => false,
            PromptMode::Confirm => true,
            PromptMode::Pin => !self.entry_text.trim().is_empty(),
            PromptMode::Passkey => passkey_entry_is_valid(&self.entry_text),
        }
    }

    fn apply_view_state(&mut self, state: &PromptViewState) {
        self.code = state.code.clone().unwrap_or_default();
        self.mode = state.mode;
        self.sync_dialog_shell(state);
    }

    fn sync_dialog_shell(&self, state: &PromptViewState) {
        self.dialog.set_heading(Some(&state.heading));
        self.dialog.set_body(&state.body);
        self.dialog
            .set_response_label(RESPONSE_CANCEL, state.cancel_label());

        if self.shows_accept() {
            if !self.dialog.has_response(RESPONSE_ACCEPT) {
                self.dialog
                    .add_response(RESPONSE_ACCEPT, self.accept_label());
            }
            self.dialog
                .set_response_label(RESPONSE_ACCEPT, self.accept_label());
            self.dialog
                .set_response_appearance(RESPONSE_ACCEPT, adw::ResponseAppearance::Suggested);
            self.dialog
                .set_response_enabled(RESPONSE_ACCEPT, self.accept_enabled());
            self.dialog.set_default_response(Some(RESPONSE_ACCEPT));
        } else {
            if self.dialog.has_response(RESPONSE_ACCEPT) {
                self.dialog.remove_response(RESPONSE_ACCEPT);
            }
            self.dialog.set_default_response(None);
        }
    }
}

struct PromptViewState {
    heading: String,
    body: String,
    code: Option<String>,
    mode: PromptMode,
}

impl PromptViewState {
    fn from_prompt(prompt: &BluetoothPrompt, snapshot: &BluetoothSnapshot) -> Self {
        let label = prompt_device_label(prompt, snapshot);
        match &prompt.kind {
            BluetoothPromptKind::Confirm { passkey } => Self {
                heading: "Confirm Pairing".into(),
                body: format!("Does the code on {label} match this one?"),
                code: Some(format!("{:06}", passkey)),
                mode: PromptMode::Confirm,
            },
            BluetoothPromptKind::AuthorizePairing => Self {
                heading: "Authorize Pairing".into(),
                body: format!("Allow {label} to pair with this computer?"),
                code: None,
                mode: PromptMode::Confirm,
            },
            BluetoothPromptKind::AuthorizeService { uuid } => Self {
                heading: "Authorize Bluetooth Service".into(),
                body: format!("Allow {label} to use Bluetooth service {uuid}?"),
                code: None,
                mode: PromptMode::Confirm,
            },
            BluetoothPromptKind::RequestPin => Self {
                heading: "Enter PIN".into(),
                body: format!("Enter the PIN shown by {label}."),
                code: None,
                mode: PromptMode::Pin,
            },
            BluetoothPromptKind::RequestPasskey => Self {
                heading: "Enter Passkey".into(),
                body: format!("Enter the passkey shown by {label}."),
                code: None,
                mode: PromptMode::Passkey,
            },
            BluetoothPromptKind::DisplayPin { pincode } => Self {
                heading: "Bluetooth Pairing".into(),
                body: format!("Type this PIN on {label} and press Enter."),
                code: Some(pincode.clone()),
                mode: PromptMode::Display,
            },
            BluetoothPromptKind::DisplayPasskey { passkey, entered } => {
                let progress = if *entered > 0 {
                    format!(" Typed on device: {entered}.")
                } else {
                    String::new()
                };

                Self {
                    heading: "Bluetooth Pairing".into(),
                    body: format!("Type this passkey on {label} and press Enter.{progress}"),
                    code: Some(format!("{:06}", passkey)),
                    mode: PromptMode::Display,
                }
            }
        }
    }

    fn cancel_label(&self) -> &'static str {
        match self.mode {
            PromptMode::Display => "Close",
            PromptMode::Confirm | PromptMode::Pin | PromptMode::Passkey => "Cancel",
        }
    }
}

fn prompt_device_label(prompt: &BluetoothPrompt, snapshot: &BluetoothSnapshot) -> String {
    if !prompt.device_label.is_empty() && prompt.device_label != prompt.device_path {
        return prompt.device_label.clone();
    }

    if let Some(address) = prompt_address(&prompt.device_path) {
        if let Some(device) = snapshot
            .devices
            .iter()
            .find(|device| device.address == address)
        {
            return device.name.clone();
        }
    }

    prompt.device_path.clone()
}

fn prompt_address(path: &str) -> Option<String> {
    let tail = path.rsplit('/').next()?;
    let suffix = tail.strip_prefix("dev_")?;
    Some(suffix.replace('_', ":"))
}

fn is_display_prompt(kind: &BluetoothPromptKind) -> bool {
    matches!(
        kind,
        BluetoothPromptKind::DisplayPin { .. } | BluetoothPromptKind::DisplayPasskey { .. }
    )
}

fn passkey_entry_is_valid(entry_text: &str) -> bool {
    parse_passkey(entry_text).is_some()
}

fn parse_passkey(entry_text: &str) -> Option<u32> {
    match entry_text.trim().parse::<u32>() {
        Ok(passkey) if passkey <= MAX_PASSKEY => Some(passkey),
        _ => None,
    }
}

fn prompt_reply_text(
    prompt: &BluetoothPrompt,
    response: &str,
    entry_text: &str,
) -> Option<BluetoothPromptReply> {
    match response {
        RESPONSE_CANCEL if is_display_prompt(&prompt.kind) => None,
        RESPONSE_CANCEL => Some(BluetoothPromptReply::Cancel),
        RESPONSE_ACCEPT => match &prompt.kind {
            BluetoothPromptKind::Confirm { .. }
            | BluetoothPromptKind::AuthorizePairing
            | BluetoothPromptKind::AuthorizeService { .. } => Some(BluetoothPromptReply::Confirm),
            BluetoothPromptKind::RequestPin => {
                let value = entry_text.trim().to_owned();
                if value.is_empty() {
                    tracing::warn!("bluetooth dialog: empty pin submitted");
                    None
                } else {
                    Some(BluetoothPromptReply::Pin(value))
                }
            }
            BluetoothPromptKind::RequestPasskey => parse_passkey(entry_text)
                .map(BluetoothPromptReply::Passkey)
                .or_else(|| {
                    tracing::warn!("bluetooth dialog: invalid passkey submitted");
                    None
                }),
            BluetoothPromptKind::DisplayPin { .. } | BluetoothPromptKind::DisplayPasskey { .. } => {
                None
            }
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::bluetooth::{BluetoothDevice, BluetoothDeviceType};

    fn snapshot_with_device(address: &str, name: &str) -> BluetoothSnapshot {
        let mut snapshot = BluetoothSnapshot::default();
        snapshot.devices.push(BluetoothDevice {
            path: String::new(),
            address: address.to_owned(),
            alias: name.to_owned(),
            name: name.to_owned(),
            device_type: BluetoothDeviceType::default(),
            paired: false,
            connected: false,
            trusted: false,
            battery: None,
            rssi: None,
            class: 0,
            appearance: 0,
            adapter: String::new(),
        });
        snapshot
    }

    #[test]
    fn prompt_uses_snapshot_device_name_when_label_is_path() {
        let prompt = BluetoothPrompt {
            id: BluetoothPromptId(7),
            device_path: "/org/bluez/hci0/dev_AA_BB_CC".into(),
            device_label: "/org/bluez/hci0/dev_AA_BB_CC".into(),
            kind: BluetoothPromptKind::RequestPin,
        };

        let snapshot = snapshot_with_device("AA:BB:CC", "Headphones");

        assert_eq!(prompt_device_label(&prompt, &snapshot), "Headphones");
    }

    #[test]
    fn prompt_reply_validates_passkeys() {
        let prompt = BluetoothPrompt {
            id: BluetoothPromptId(1),
            device_path: String::new(),
            device_label: String::new(),
            kind: BluetoothPromptKind::RequestPasskey,
        };
        assert_eq!(
            prompt_reply_text(&prompt, RESPONSE_ACCEPT, "not-a-number"),
            None
        );
        assert_eq!(
            prompt_reply_text(&prompt, RESPONSE_ACCEPT, "123456"),
            Some(BluetoothPromptReply::Passkey(123456))
        );
        assert_eq!(prompt_reply_text(&prompt, RESPONSE_ACCEPT, "1000000"), None);
    }

    #[test]
    fn passkey_accept_validation_matches_reply_range() {
        assert!(!passkey_entry_is_valid("not-a-number"));
        assert!(passkey_entry_is_valid("123456"));
        assert!(!passkey_entry_is_valid("1000000"));
    }

    #[test]
    fn display_prompt_close_is_local_only() {
        let prompt = BluetoothPrompt {
            id: BluetoothPromptId(1),
            device_path: String::new(),
            device_label: String::new(),
            kind: BluetoothPromptKind::DisplayPin {
                pincode: "1234".into(),
            },
        };

        assert_eq!(prompt_reply_text(&prompt, RESPONSE_CANCEL, ""), None);
    }
}
