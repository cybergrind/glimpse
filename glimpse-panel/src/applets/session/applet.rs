use adw::prelude::{AdwDialogExt, AlertDialogExt};
use glimpse::providers::session_actions::{SessionActions, SessionSnapshot};
use relm4::{
    gtk::{self, prelude::*},
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
};

use super::config::SessionConfig;
use super::popover::{
    SessionAction, SessionPopover, SessionPopoverInit, SessionPopoverInput, SessionPopoverOutput,
};

pub struct Session {
    config: SessionConfig,
    conn: zbus::Connection,
    label: String,
    popover: Controller<SessionPopover>,
}

pub struct SessionInit {
    pub config: SessionConfig,
    pub conn: zbus::Connection,
}

#[derive(Debug)]
pub enum SessionMsg {
    TogglePopover,
    SnapshotLoaded(SessionSnapshot),
    SnapshotUnavailable,
    PopoverOutput(SessionPopoverOutput),
    Confirmed(SessionAction),
}

#[derive(Debug, Clone, Copy)]
struct ConfirmationSpec {
    heading: &'static str,
    body: &'static str,
    accept_label: &'static str,
    suggested: bool,
}

#[relm4::component(pub)]
impl Component for Session {
    type Init = SessionInit;
    type Input = SessionMsg;
    type Output = ();
    type CommandOutput = SessionMsg;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "hoverable",
            add_css_class: "session",
            set_tooltip_text: Some("Session"),

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(SessionMsg::TogglePopover);
                }
            },

            gtk::Label {
                #[watch]
                set_label: &model.label,
                add_css_class: "session-label",
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover_config = init.config.clone();
        let popover = SessionPopover::builder()
            .launch(SessionPopoverInit {
                parent: root.clone(),
                config: popover_config,
            })
            .forward(sender.input_sender(), SessionMsg::PopoverOutput);

        let conn = init.conn;
        let model = Session {
            config: init.config,
            conn: conn.clone(),
            label: std::env::var("USER").unwrap_or_else(|_| "user".into()),
            popover,
        };

        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    let provider = SessionActions::with_connection(conn);
                    let msg = match provider.snapshot().await {
                        Ok(snapshot) => SessionMsg::SnapshotLoaded(snapshot),
                        Err(error) => {
                            tracing::warn!(error = %error, "session applet: failed to load snapshot");
                            SessionMsg::SnapshotUnavailable
                        }
                    };
                    let _ = out.send(msg);
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            SessionMsg::TogglePopover => {
                self.popover.emit(SessionPopoverInput::Toggle);
            }
            SessionMsg::SnapshotLoaded(snapshot) => {
                self.label = user_label(&snapshot.user_name);
                self.popover.emit(SessionPopoverInput::Update(snapshot));
            }
            SessionMsg::SnapshotUnavailable => {
                self.label = std::env::var("USER").unwrap_or_else(|_| "user".into());
                self.popover
                    .emit(SessionPopoverInput::Update(SessionSnapshot::default()));
            }
            SessionMsg::PopoverOutput(SessionPopoverOutput::ActionRequested(action)) => {
                if let Some(spec) = confirmation_spec(action, &self.config) {
                    self.popover.emit(SessionPopoverInput::Close);
                    show_confirmation(root, &sender, action, spec);
                } else {
                    self.popover.emit(SessionPopoverInput::Close);
                    run_action(self.conn.clone(), action);
                }
            }
            SessionMsg::Confirmed(action) => {
                self.popover.emit(SessionPopoverInput::Close);
                run_action(self.conn.clone(), action);
            }
        }
    }
}

fn user_label(user_name: &str) -> String {
    let trimmed = user_name.trim();
    if trimmed.is_empty() {
        std::env::var("USER").unwrap_or_else(|_| "user".into())
    } else {
        trimmed.to_owned()
    }
}

fn confirmation_spec(action: SessionAction, config: &SessionConfig) -> Option<ConfirmationSpec> {
    match action {
        SessionAction::Lock => None,
        SessionAction::Logout if config.confirm_logout => Some(ConfirmationSpec {
            heading: "Log Out",
            body: "End the current session and log out now?",
            accept_label: "Log Out",
            suggested: false,
        }),
        SessionAction::Suspend if config.confirm_suspend => Some(ConfirmationSpec {
            heading: "Suspend",
            body: "Suspend the system now?",
            accept_label: "Suspend",
            suggested: false,
        }),
        SessionAction::Hibernate if config.confirm_hibernate => Some(ConfirmationSpec {
            heading: "Hibernate",
            body: "Hibernate the system now?",
            accept_label: "Hibernate",
            suggested: false,
        }),
        SessionAction::Reboot if config.confirm_reboot => Some(ConfirmationSpec {
            heading: "Restart",
            body: "Restart the system now?",
            accept_label: "Restart",
            suggested: false,
        }),
        SessionAction::PowerOff if config.confirm_shutdown => Some(ConfirmationSpec {
            heading: "Shut Down",
            body: "Shut down the system now?",
            accept_label: "Shut Down",
            suggested: false,
        }),
        _ => None,
    }
}

fn show_confirmation(
    root: &gtk::Box,
    sender: &ComponentSender<Session>,
    action: SessionAction,
    spec: ConfirmationSpec,
) {
    let dialog = adw::AlertDialog::new(Some(spec.heading), Some(spec.body));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("accept", spec.accept_label);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("accept"));
    dialog.set_response_appearance(
        "accept",
        if spec.suggested {
            adw::ResponseAppearance::Suggested
        } else {
            adw::ResponseAppearance::Destructive
        },
    );

    let sender = sender.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "accept" {
            sender.input(SessionMsg::Confirmed(action));
        }
    });

    dialog.present(Some(root));
}

fn run_action(conn: zbus::Connection, action: SessionAction) {
    relm4::spawn(async move {
        let provider = SessionActions::with_connection(conn);
        let result = match action {
            SessionAction::Lock => provider.lock().await,
            SessionAction::Logout => provider.logout().await,
            SessionAction::Suspend => provider.suspend().await,
            SessionAction::Hibernate => provider.hibernate().await,
            SessionAction::Reboot => provider.reboot().await,
            SessionAction::PowerOff => provider.power_off().await,
        };

        if let Err(error) = result {
            tracing::warn!(?action, error = %error, "session applet: action failed");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_screen_does_not_require_confirmation() {
        assert!(confirmation_spec(SessionAction::Lock, &SessionConfig::default()).is_none());
    }

    #[test]
    fn session_ending_actions_require_confirmation() {
        let config = SessionConfig::default();
        for action in [
            SessionAction::Logout,
            SessionAction::Suspend,
            SessionAction::Hibernate,
            SessionAction::Reboot,
            SessionAction::PowerOff,
        ] {
            let spec = confirmation_spec(action, &config).expect("confirmation spec");
            assert!(!spec.heading.is_empty());
            assert!(!spec.body.is_empty());
            assert!(!spec.accept_label.is_empty());
        }
    }

    #[test]
    fn confirmation_settings_are_respected() {
        let config = SessionConfig {
            confirm_logout: false,
            confirm_suspend: false,
            confirm_hibernate: false,
            confirm_reboot: false,
            confirm_shutdown: false,
            ..SessionConfig::default()
        };

        assert!(confirmation_spec(SessionAction::Logout, &config).is_none());
        assert!(confirmation_spec(SessionAction::Suspend, &config).is_none());
        assert!(confirmation_spec(SessionAction::Hibernate, &config).is_none());
        assert!(confirmation_spec(SessionAction::Reboot, &config).is_none());
        assert!(confirmation_spec(SessionAction::PowerOff, &config).is_none());
    }

    #[test]
    fn blank_user_name_uses_a_fallback_label() {
        assert_eq!(user_label("alice"), "alice");
        assert!(!user_label("   ").is_empty());
    }
}
