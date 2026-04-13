use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use glimpse::bluetooth::{
    protocol::{BluetoothPrompt, BluetoothPromptId, BluetoothPromptKind, BluetoothPromptReply},
    provider::BluetoothSnapshot,
};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self},
};

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_ACCEPT: &str = "accept";

pub struct BluetoothPromptDialogInit {
    pub parent: gtk::Widget,
}

pub struct BluetoothPromptDialog {
    parent: gtk::Widget,
    dialog: adw::AlertDialog,
    current_prompt: Rc<RefCell<Option<BluetoothPrompt>>>,
    generation: Rc<Cell<u64>>,
    code: String,
    entry_text: String,
    mode: BluetoothPromptMode,
    entry: gtk::Entry,
}

#[derive(Debug, Clone)]
pub enum BluetoothPromptDialogInput {
    Update {
        prompt: Option<BluetoothPrompt>,
        snapshot: BluetoothSnapshot,
    },
    EntryChanged(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothPromptDialogOutput {
    Reply {
        id: BluetoothPromptId,
        reply: BluetoothPromptReply,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum BluetoothPromptMode {
    #[default]
    Display,
    Confirm,
    Pin,
    Passkey,
}

#[relm4::component(pub)]
impl SimpleComponent for BluetoothPromptDialog {
    type Init = BluetoothPromptDialogInit;
    type Input = BluetoothPromptDialogInput;
    type Output = BluetoothPromptDialogOutput;

    view! {
        gtk::Box {
            add_css_class: "bt-prompt-content",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,

            gtk::Label {
                add_css_class: "bt-prompt-code",
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
                add_css_class: "bt-prompt-entry",
                #[watch]
                set_visible: model.shows_entry(),
                #[watch]
                set_placeholder_text: Some(model.entry_placeholder()),
                set_input_purpose: gtk::InputPurpose::Digits,
                connect_changed[sender] => move |entry| {
                    sender.input(BluetoothPromptDialogInput::EntryChanged(entry.text().to_string()));
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
        dialog.set_extra_child(Some(&root));

        let model = BluetoothPromptDialog {
            parent: init.parent,
            dialog,
            current_prompt: Rc::new(RefCell::new(None)),
            generation: Rc::new(Cell::new(0)),
            code: String::new(),
            entry_text: String::new(),
            mode: BluetoothPromptMode::Display,
            entry: gtk::Entry::new(),
        };

        let widgets = view_output!();
        let mut model = model;
        model.entry = widgets.entry.clone();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BluetoothPromptDialogInput::Update { prompt, snapshot } => {
                let Some(prompt) = prompt else {
                    self.entry_text.clear();
                    *self.current_prompt.borrow_mut() = None;
                    self.generation.set(self.generation.get().wrapping_add(1));
                    self.dialog.force_close();
                    return;
                };

                if self.current_prompt.borrow().as_ref() == Some(&prompt) {
                    return;
                }

                self.dialog.force_close();

                let state = BluetoothPromptViewState::from_prompt(&prompt, &snapshot);
                self.code = state.code.clone().unwrap_or_default();
                self.entry_text.clear();
                self.mode = state.mode;
                self.sync_dialog_shell(&state);

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
                let response_generation = self.generation.clone();
                let expected_prompt = prompt;

                relm4::spawn_local(async move {
                    let response = response_dialog.choose_future(&response_parent).await;
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

                    let reply = bluetooth_prompt_reply_text(
                        &active_prompt,
                        response.as_str(),
                        response_entry.text().as_str(),
                    );
                    if let Some(reply) = reply {
                        let _ = response_sender.output(BluetoothPromptDialogOutput::Reply {
                            id: active_prompt.id,
                            reply,
                        });
                    }
                });

                if self.shows_entry() {
                    self.entry.grab_focus();
                }
            }
            BluetoothPromptDialogInput::EntryChanged(text) => {
                self.entry_text = text;
                if self.dialog.has_response(RESPONSE_ACCEPT) {
                    self.dialog
                        .set_response_enabled(RESPONSE_ACCEPT, self.accept_enabled());
                }
            }
        }
    }
}

impl BluetoothPromptDialog {
    fn shows_code(&self) -> bool {
        !self.code.is_empty()
    }

    fn shows_entry(&self) -> bool {
        matches!(self.mode, BluetoothPromptMode::Pin | BluetoothPromptMode::Passkey)
    }

    fn shows_accept(&self) -> bool {
        !matches!(self.mode, BluetoothPromptMode::Display)
    }

    fn entry_placeholder(&self) -> &'static str {
        match self.mode {
            BluetoothPromptMode::Pin => "PIN",
            BluetoothPromptMode::Passkey => "Passkey",
            BluetoothPromptMode::Display | BluetoothPromptMode::Confirm => "",
        }
    }

    fn accept_label(&self) -> &'static str {
        match self.mode {
            BluetoothPromptMode::Confirm => "Pair",
            BluetoothPromptMode::Pin => "Submit PIN",
            BluetoothPromptMode::Passkey => "Submit Passkey",
            BluetoothPromptMode::Display => "",
        }
    }

    fn accept_enabled(&self) -> bool {
        match self.mode {
            BluetoothPromptMode::Display => false,
            BluetoothPromptMode::Confirm => true,
            BluetoothPromptMode::Pin => !self.entry_text.trim().is_empty(),
            BluetoothPromptMode::Passkey => self.entry_text.trim().parse::<u32>().is_ok(),
        }
    }

    fn sync_dialog_shell(&self, state: &BluetoothPromptViewState) {
        self.dialog.set_heading(Some(&state.heading));
        self.dialog.set_body(&state.body);
        self.dialog
            .set_response_label(RESPONSE_CANCEL, state.cancel_label());

        if self.shows_accept() {
            if !self.dialog.has_response(RESPONSE_ACCEPT) {
                self.dialog.add_response(RESPONSE_ACCEPT, self.accept_label());
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

struct BluetoothPromptViewState {
    heading: String,
    body: String,
    code: Option<String>,
    mode: BluetoothPromptMode,
}

impl BluetoothPromptViewState {
    fn from_prompt(prompt: &BluetoothPrompt, snapshot: &BluetoothSnapshot) -> Self {
        let label = bluetooth_prompt_device_label(prompt, snapshot);
        match &prompt.kind {
            BluetoothPromptKind::Confirm { passkey } => Self {
                heading: "Confirm Pairing".into(),
                body: format!("Does the code on {label} match this one?"),
                code: Some(format!("{:06}", passkey)),
                mode: BluetoothPromptMode::Confirm,
            },
            BluetoothPromptKind::RequestPin => Self {
                heading: "Enter PIN".into(),
                body: format!("Enter the PIN shown by {label}."),
                code: None,
                mode: BluetoothPromptMode::Pin,
            },
            BluetoothPromptKind::RequestPasskey => Self {
                heading: "Enter Passkey".into(),
                body: format!("Enter the passkey shown by {label}."),
                code: None,
                mode: BluetoothPromptMode::Passkey,
            },
            BluetoothPromptKind::DisplayPin { pincode } => Self {
                heading: "Bluetooth Pairing".into(),
                body: format!("Type this PIN on {label} and press Enter."),
                code: Some(pincode.clone()),
                mode: BluetoothPromptMode::Display,
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
                    mode: BluetoothPromptMode::Display,
                }
            }
        }
    }

    fn cancel_label(&self) -> &'static str {
        match self.mode {
            BluetoothPromptMode::Display => "Close",
            BluetoothPromptMode::Confirm
            | BluetoothPromptMode::Pin
            | BluetoothPromptMode::Passkey => "Cancel",
        }
    }
}

fn bluetooth_prompt_device_label(prompt: &BluetoothPrompt, snapshot: &BluetoothSnapshot) -> String {
    if !prompt.device_label.is_empty() && prompt.device_label != prompt.device_path {
        return prompt.device_label.clone();
    }

    if let Some(address) = bluetooth_prompt_address(&prompt.device_path) {
        if let Some(device) = snapshot.devices.iter().find(|device| device.address == address) {
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

fn bluetooth_prompt_reply_text(
    prompt: &BluetoothPrompt,
    response: &str,
    entry_text: &str,
) -> Option<BluetoothPromptReply> {
    match response {
        RESPONSE_CANCEL => Some(BluetoothPromptReply::Cancel),
        RESPONSE_ACCEPT => match &prompt.kind {
            BluetoothPromptKind::Confirm { .. } => Some(BluetoothPromptReply::Confirm),
            BluetoothPromptKind::RequestPin => {
                let value = entry_text.trim().to_owned();
                if value.is_empty() {
                    tracing::warn!("bluetooth dialog: empty pin submitted");
                    None
                } else {
                    Some(BluetoothPromptReply::Pin(value))
                }
            }
            BluetoothPromptKind::RequestPasskey => match entry_text.trim().parse::<u32>() {
                Ok(passkey) => Some(BluetoothPromptReply::Passkey(passkey)),
                Err(error) => {
                    tracing::warn!(error = %error, value = entry_text, "bluetooth dialog: invalid passkey submitted");
                    None
                }
            },
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
    use glimpse::bluetooth::provider::{BluetoothDevice, BluetoothDeviceType};

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

        assert_eq!(bluetooth_prompt_device_label(&prompt, &snapshot), "Headphones");
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
            bluetooth_prompt_reply_text(&prompt, RESPONSE_ACCEPT, "not-a-number"),
            None
        );
        assert_eq!(
            bluetooth_prompt_reply_text(&prompt, RESPONSE_ACCEPT, "123456"),
            Some(BluetoothPromptReply::Passkey(123456))
        );
    }
}
