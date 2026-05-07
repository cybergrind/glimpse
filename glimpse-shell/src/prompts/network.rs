#![allow(unused_assignments)]

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

use crate::agents::network::{
    NetworkAgentHandle, NetworkPrompt, NetworkPromptId, NetworkPromptReply, Secret,
};

const RESPONSE_CANCEL: &str = "cancel";
const RESPONSE_ACCEPT: &str = "accept";

pub struct PromptHost {
    agent: NetworkAgentHandle,
    dialog: Controller<PromptDialog>,
    subscription_cancel: CancellationToken,
}

pub struct PromptHostInit {
    pub agent: NetworkAgentHandle,
    pub parent: gtk::Widget,
}

#[derive(Debug)]
pub enum PromptHostInput {
    SetParent(gtk::Widget),
    DialogOutput(PromptDialogOutput),
}

#[relm4::component(pub)]
impl Component for PromptHost {
    type Init = PromptHostInit;
    type Input = PromptHostInput;
    type Output = ();
    type CommandOutput = Option<NetworkPrompt>;

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
        self.dialog
            .emit(PromptDialogInput::Update { prompt: state });
    }
}

impl PromptHost {
    fn send_reply(&self, id: NetworkPromptId, reply: NetworkPromptReply) {
        if !self.agent.reply(id, reply) {
            tracing::warn!(prompt_id = id.0, "failed to send network prompt reply");
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
}

pub struct PromptDialog {
    parent: gtk::Widget,
    dialog: adw::AlertDialog,
    current_prompt: Rc<RefCell<Option<NetworkPrompt>>>,
    generation: Rc<Cell<u64>>,
    entry_text: String,
    entry: gtk::Entry,
}

#[derive(Debug, Clone)]
pub enum PromptDialogInput {
    Update { prompt: Option<NetworkPrompt> },
    SetParent(gtk::Widget),
    EntryChanged(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptDialogOutput {
    Reply {
        id: NetworkPromptId,
        reply: NetworkPromptReply,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for PromptDialog {
    type Init = PromptDialogInit;
    type Input = PromptDialogInput;
    type Output = PromptDialogOutput;

    view! {
        gtk::Box {
            add_css_class: "network-prompt",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,

            #[name(entry)]
            gtk::Entry {
                add_css_class: "network-prompt__entry",
                set_visibility: false,
                set_activates_default: true,
                set_placeholder_text: Some("Password"),
                set_input_purpose: gtk::InputPurpose::Password,
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
        let dialog = adw::AlertDialog::new(Some("Wi-Fi Password"), Some(""));
        dialog.add_response(RESPONSE_CANCEL, "Cancel");
        dialog.add_response(RESPONSE_ACCEPT, "Connect");
        dialog.set_close_response(RESPONSE_CANCEL);
        dialog.set_default_response(Some(RESPONSE_ACCEPT));
        dialog.set_response_appearance(RESPONSE_ACCEPT, adw::ResponseAppearance::Suggested);
        dialog.set_response_enabled(RESPONSE_ACCEPT, false);
        dialog.set_extra_child(Some(&root));

        let model = PromptDialog {
            parent: init.parent,
            dialog,
            current_prompt: Rc::new(RefCell::new(None)),
            generation: Rc::new(Cell::new(0)),
            entry_text: String::new(),
            entry: gtk::Entry::new(),
        };

        let widgets = view_output!();
        let mut model = model;
        model.entry = widgets.entry.clone();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PromptDialogInput::Update { prompt } => {
                let reset_form =
                    should_reset_form(self.current_prompt.borrow().as_ref(), prompt.as_ref());

                let Some(prompt) = prompt else {
                    self.entry_text.clear();
                    *self.current_prompt.borrow_mut() = None;
                    self.generation.set(self.generation.get().wrapping_add(1));
                    self.dialog.force_close();
                    return;
                };

                if reset_form {
                    self.clear_form();
                    self.dialog.force_close();
                }

                self.dialog.set_heading(Some("Wi-Fi Password"));
                self.dialog.set_body(&prompt_body(&prompt));
                self.dialog
                    .set_response_enabled(RESPONSE_ACCEPT, self.accept_enabled());

                if !reset_form {
                    *self.current_prompt.borrow_mut() = Some(prompt);
                    return;
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

                relm4::spawn_local(async move {
                    let response = response_dialog.choose_future(&response_parent).await;
                    let active_prompt = response_prompt.borrow().clone();

                    let Some(active_prompt) = active_prompt else {
                        return;
                    };

                    if !response_generation_is_current(response_generation.get(), generation) {
                        return;
                    }

                    *response_prompt.borrow_mut() = None;

                    let reply = match response.as_str() {
                        RESPONSE_CANCEL => Some(NetworkPromptReply::Cancel),
                        RESPONSE_ACCEPT => password_reply(response_entry.text().as_str()),
                        _ => None,
                    };

                    if let Some(reply) = reply {
                        let _ = response_sender.output(PromptDialogOutput::Reply {
                            id: active_prompt.id,
                            reply,
                        });
                    }
                });

                self.dialog.present(Some(&self.parent));
                self.entry.grab_focus();
            }
            PromptDialogInput::SetParent(parent) => {
                self.parent = parent;
            }
            PromptDialogInput::EntryChanged(text) => {
                self.entry_text = text;
                self.dialog
                    .set_response_enabled(RESPONSE_ACCEPT, self.accept_enabled());
            }
        }
    }
}

impl PromptDialog {
    fn accept_enabled(&self) -> bool {
        !self.entry_text.trim().is_empty()
    }

    fn clear_form(&mut self) {
        if !self.entry.text().is_empty() {
            self.entry.set_text("");
        }
        self.entry.set_position(-1);
        self.entry_text.clear();
    }
}

fn prompt_body(prompt: &NetworkPrompt) -> String {
    format!("Enter the password for {}.", prompt.ssid)
}

fn password_reply(value: &str) -> Option<NetworkPromptReply> {
    let value = value.trim();
    if value.is_empty() {
        tracing::warn!("network dialog: empty password submitted");
        Some(NetworkPromptReply::Cancel)
    } else {
        Some(NetworkPromptReply::Password(Secret::new(value)))
    }
}

fn should_reset_form(
    current_prompt: Option<&NetworkPrompt>,
    next_prompt: Option<&NetworkPrompt>,
) -> bool {
    let Some(next_prompt) = next_prompt else {
        return true;
    };
    let Some(current_prompt) = current_prompt else {
        return true;
    };

    current_prompt.ssid != next_prompt.ssid
}

fn response_generation_is_current(current: u64, expected: u64) -> bool {
    current == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::network::{NetworkPrompt, NetworkPromptId, NetworkPromptReply};

    fn prompt(id: u64, ssid: &str) -> NetworkPrompt {
        NetworkPrompt {
            id: NetworkPromptId(id),
            ssid: ssid.into(),
        }
    }

    #[test]
    fn prompt_body_names_wifi_network() {
        assert_eq!(
            prompt_body(&prompt(1, "Office")),
            "Enter the password for Office."
        );
    }

    #[test]
    fn password_reply_cancels_empty_values() {
        assert_eq!(password_reply("  "), Some(NetworkPromptReply::Cancel));
        assert_eq!(
            password_reply("secret"),
            Some(NetworkPromptReply::Password(Secret::new("secret")))
        );
    }

    #[test]
    fn prompt_form_is_preserved_for_same_ssid() {
        let current = prompt(1, "Office");
        let next = prompt(2, "Office");

        assert!(!should_reset_form(Some(&current), Some(&next)));
    }

    #[test]
    fn same_ssid_prompt_update_keeps_response_generation_current() {
        assert!(response_generation_is_current(2, 2));
        assert!(!response_generation_is_current(3, 2));
    }

    #[test]
    fn prompt_form_resets_for_different_ssid_or_missing_prompt() {
        let current = prompt(1, "Office");

        assert!(should_reset_form(Some(&current), Some(&prompt(2, "Guest"))));
        assert!(should_reset_form(Some(&current), None));
        assert!(should_reset_form(None, Some(&current)));
    }
}
