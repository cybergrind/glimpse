use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use glimpse::network::protocol::{NetworkPrompt, NetworkPromptId, NetworkPromptKind, NetworkPromptReply};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self},
};

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_ACCEPT: &str = "accept";

pub struct NetworkPromptDialogInit {
    pub parent: gtk::Widget,
}

pub struct NetworkPromptDialog {
    parent: gtk::Widget,
    dialog: adw::AlertDialog,
    current_prompt: Rc<RefCell<Option<NetworkPrompt>>>,
    entry_text: String,
    error_text: String,
    submitting: bool,
    entry: gtk::Entry,
}

#[derive(Debug, Clone)]
pub enum NetworkPromptDialogInput {
    Update { prompt: Option<NetworkPrompt> },
    EntryChanged(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkPromptDialogOutput {
    Reply {
        id: NetworkPromptId,
        reply: NetworkPromptReply,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for NetworkPromptDialog {
    type Init = NetworkPromptDialogInit;
    type Input = NetworkPromptDialogInput;
    type Output = NetworkPromptDialogOutput;

    view! {
        gtk::Box {
            add_css_class: "net-prompt-content",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,

            #[name(entry)]
            gtk::Entry {
                add_css_class: "net-prompt-entry",
                set_visibility: false,
                set_activates_default: false,
                set_placeholder_text: Some("Password"),
                set_input_purpose: gtk::InputPurpose::Password,
                #[watch]
                set_sensitive: !model.submitting,
                connect_changed[sender] => move |entry| {
                    sender.input(NetworkPromptDialogInput::EntryChanged(entry.text().to_string()));
                },
            },

            gtk::Label {
                add_css_class: "error",
                #[watch]
                set_visible: !model.error_text.is_empty(),
                #[watch]
                set_label: &model.error_text,
                set_halign: gtk::Align::Start,
                set_xalign: 0.0,
                set_wrap: true,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let dialog = adw::AlertDialog::new(Some("Wi-Fi Password"), Some(""));
        dialog.add_response(RESPONSE_CANCEL, "Cancel");
        dialog.add_response(RESPONSE_ACCEPT, "Connect");
        dialog.set_close_response(RESPONSE_CANCEL);
        dialog.set_default_response(Some(RESPONSE_ACCEPT));
        dialog.set_response_appearance(RESPONSE_ACCEPT, adw::ResponseAppearance::Suggested);
        dialog.set_response_enabled(RESPONSE_ACCEPT, false);
        dialog.set_extra_child(Some(&root));

        let model = NetworkPromptDialog {
            parent: init.parent,
            dialog,
            current_prompt: Rc::new(RefCell::new(None)),
            entry_text: String::new(),
            error_text: String::new(),
            submitting: false,
            entry: gtk::Entry::new(),
        };

        let widgets = view_output!();
        let mut model = model;
        model.entry = widgets.entry.clone();

        let response_prompt = model.current_prompt.clone();
        let response_entry = model.entry.clone();
        let response_sender = sender.clone();
        model.dialog.connect_response(None, move |_, response| {
            let Some(active_prompt) = response_prompt.borrow().clone() else {
                return;
            };

            if active_prompt.submitting {
                return;
            }

            let reply = match response {
                RESPONSE_CANCEL => Some(NetworkPromptReply::Cancel),
                RESPONSE_ACCEPT => network_submit_prompt_reply_text(response_entry.text().as_str()),
                _ => None,
            };

            if let Some(reply) = reply {
                *response_prompt.borrow_mut() = None;
                let _ = response_sender.output(NetworkPromptDialogOutput::Reply {
                    id: active_prompt.id,
                    reply,
                });
            }
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NetworkPromptDialogInput::Update { prompt } => {
                let reset_form =
                    should_reset_network_prompt_form(self.current_prompt.borrow().as_ref(), prompt.as_ref());

                let Some(prompt) = prompt else {
                    self.entry_text.clear();
                    self.error_text.clear();
                    self.submitting = false;
                    *self.current_prompt.borrow_mut() = None;
                    self.clear_form();
                    self.dialog.force_close();
                    return;
                };

                if reset_form {
                    self.clear_form();
                }

                self.error_text = prompt.error_message.clone().unwrap_or_default();
                self.submitting = prompt.submitting;
                *self.current_prompt.borrow_mut() = Some(prompt.clone());
                self.sync_dialog_shell(&prompt);

                if reset_form {
                    self.dialog.present(Some(&self.parent));
                }

                if !self.submitting {
                    self.entry.grab_focus();
                }
            }
            NetworkPromptDialogInput::EntryChanged(text) => {
                self.entry_text = text;
                self.dialog
                    .set_response_enabled(RESPONSE_ACCEPT, self.accept_enabled());
            }
        }
    }
}

impl NetworkPromptDialog {
    fn accept_enabled(&self) -> bool {
        !self.submitting && !self.entry_text.trim().is_empty()
    }

    fn clear_form(&mut self) {
        self.entry.set_text("");
        self.entry.set_position(-1);
        self.entry_text.clear();
    }

    fn sync_dialog_shell(&self, prompt: &NetworkPrompt) {
        let body = match &prompt.kind {
            NetworkPromptKind::WifiPassword { ssid } => format!("Enter the password for {ssid}."),
        };

        self.dialog.set_heading(Some("Wi-Fi Password"));
        self.dialog.set_body(&body);
        self.dialog.set_can_close(!prompt.submitting);
        self.dialog
            .set_response_enabled(RESPONSE_CANCEL, !prompt.submitting);
        self.dialog
            .set_response_enabled(RESPONSE_ACCEPT, self.accept_enabled());
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

fn should_reset_network_prompt_form(
    current_prompt: Option<&NetworkPrompt>,
    next_prompt: Option<&NetworkPrompt>,
) -> bool {
    let Some(next_prompt) = next_prompt else {
        return true;
    };

    !should_update_network_prompt_in_place(current_prompt, next_prompt)
}

fn network_submit_prompt_reply_text(value: &str) -> Option<NetworkPromptReply> {
    let value = value.trim().to_string();
    if value.is_empty() {
        tracing::warn!("network dialog: empty password submitted");
        None
    } else {
        Some(NetworkPromptReply::SubmitPassword(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network_prompt(id: u64, ssid: &str) -> NetworkPrompt {
        NetworkPrompt {
            id: NetworkPromptId(id),
            kind: NetworkPromptKind::WifiPassword { ssid: ssid.into() },
            error_message: None,
            submitting: false,
        }
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

        assert!(!should_reset_network_prompt_form(Some(&current), Some(&next)));
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

        assert!(!should_update_network_prompt_in_place(Some(&current), &next));
    }

    #[test]
    fn wifi_password_prompt_rebuilds_without_current_prompt() {
        let next = network_prompt(1, "Skylink");

        assert!(should_reset_network_prompt_form(None, Some(&next)));
    }

    #[test]
    fn wifi_password_prompt_rebuilds_when_kind_disappears() {
        let current = network_prompt(1, "Skylink");

        assert!(should_reset_network_prompt_form(Some(&current), None));
    }

    #[test]
    fn submit_password_rejects_empty_values() {
        assert_eq!(network_submit_prompt_reply_text("  "), None);
        assert_eq!(
            network_submit_prompt_reply_text("secret"),
            Some(NetworkPromptReply::SubmitPassword("secret".into()))
        );
    }
}
