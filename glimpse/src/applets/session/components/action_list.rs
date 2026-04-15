use glimpse::session_actions::provider::{
    SessionActionAvailability, SessionBackendState, SessionSnapshot,
};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::super::{SessionConfig, popover::SessionAction};

pub struct SessionActionList {
    config: SessionConfig,
    rows: Vec<SessionActionRowView>,
    container: gtk::Box,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionActionRowView {
    pub action: SessionAction,
    pub icon_name: &'static str,
    pub label: &'static str,
    pub enabled: bool,
    pub separated: bool,
}

#[derive(Debug)]
pub enum SessionActionListInput {
    Update(SessionSnapshot),
    Reconfigure {
        config: SessionConfig,
        snapshot: SessionSnapshot,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionActionListOutput {
    ActionRequested(SessionAction),
}

#[relm4::component(pub)]
impl SimpleComponent for SessionActionList {
    type Init = SessionConfig;
    type Input = SessionActionListInput;
    type Output = SessionActionListOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let rows = build_action_rows(&init, &SessionSnapshot::default());
        render_action_rows(&root, &rows, &sender);

        let model = SessionActionList {
            config: init,
            rows,
            container: root.clone(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        let snapshot = match message {
            SessionActionListInput::Update(snapshot) => snapshot,
            SessionActionListInput::Reconfigure { config, snapshot } => {
                self.config = config;
                snapshot
            }
        };
        self.rows = build_action_rows(&self.config, &snapshot);
        render_action_rows(&self.container, &self.rows, &sender);
    }
}

pub fn build_action_rows(
    config: &SessionConfig,
    snapshot: &SessionSnapshot,
) -> Vec<SessionActionRowView> {
    let caps = &snapshot.capabilities;
    let mut rows = Vec::new();

    if config.show_lock {
        rows.push(SessionActionRowView {
            action: SessionAction::Lock,
            icon_name: "system-lock-screen-symbolic",
            label: "Lock Screen",
            enabled: action_enabled(&caps.lock),
            separated: false,
        });
    }

    if config.show_logout {
        rows.push(SessionActionRowView {
            action: SessionAction::Logout,
            icon_name: "system-log-out-symbolic",
            label: "Log Out",
            enabled: matches!(caps.backend, SessionBackendState::Available),
            separated: false,
        });
    }

    let has_session_rows = !rows.is_empty();

    if config.show_suspend {
        rows.push(SessionActionRowView {
            action: SessionAction::Suspend,
            icon_name: "media-playback-pause-symbolic",
            label: "Suspend",
            enabled: action_enabled(&caps.suspend),
            separated: has_session_rows,
        });
    }

    if config.show_hibernate {
        rows.push(SessionActionRowView {
            action: SessionAction::Hibernate,
            icon_name: "document-save-symbolic",
            label: "Hibernate",
            enabled: action_enabled(&caps.hibernate),
            separated: has_session_rows && !rows.iter().any(|row| row.separated),
        });
    }

    if config.show_reboot {
        rows.push(SessionActionRowView {
            action: SessionAction::Reboot,
            icon_name: "system-reboot-symbolic",
            label: "Restart",
            enabled: action_enabled(&caps.reboot),
            separated: has_session_rows && !rows.iter().any(|row| row.separated),
        });
    }

    if config.show_shutdown {
        rows.push(SessionActionRowView {
            action: SessionAction::PowerOff,
            icon_name: "system-shutdown-symbolic",
            label: "Shut Down",
            enabled: action_enabled(&caps.power_off),
            separated: has_session_rows && !rows.iter().any(|row| row.separated),
        });
    }

    rows
}

fn action_enabled(value: &SessionActionAvailability) -> bool {
    matches!(
        value,
        SessionActionAvailability::Available | SessionActionAvailability::Challenge
    )
}

fn render_action_rows(
    container: &gtk::Box,
    rows: &[SessionActionRowView],
    sender: &ComponentSender<SessionActionList>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    for row in rows.iter().cloned() {
        if row.separated {
            container.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        container.append(&build_action_row(row, sender));
    }
}

fn build_action_row(
    row: SessionActionRowView,
    sender: &ComponentSender<SessionActionList>,
) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.add_css_class("session-action-row");

    let icon = gtk::Image::from_icon_name(row.icon_name);
    icon.set_pixel_size(16);
    icon.add_css_class("session-action-icon");
    content.append(&icon);

    let label = gtk::Label::new(Some(row.label));
    label.set_hexpand(true);
    label.set_halign(gtk::Align::Start);
    content.append(&label);

    let button = gtk::Button::new();
    button.set_child(Some(&content));
    button.add_css_class("flat");
    button.add_css_class("session-action-btn");
    button.set_sensitive(row.enabled);

    let action = row.action;
    let sender = sender.clone();
    button.connect_clicked(move |_| {
        let _ = sender.output(SessionActionListOutput::ActionRequested(action));
    });

    button
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::session_actions::provider::SessionActionCapabilities;

    #[test]
    fn action_enabled_treats_available_and_challenge_as_enabled() {
        assert!(action_enabled(&SessionActionAvailability::Available));
        assert!(action_enabled(&SessionActionAvailability::Challenge));
        assert!(!action_enabled(&SessionActionAvailability::Unavailable));
    }

    #[test]
    fn build_action_rows_honors_hidden_actions() {
        let config = SessionConfig {
            show_hibernate: false,
            ..SessionConfig::default()
        };
        let snapshot = SessionSnapshot {
            capabilities: SessionActionCapabilities {
                backend: SessionBackendState::Available,
                suspend: SessionActionAvailability::Available,
                hibernate: SessionActionAvailability::Available,
                reboot: SessionActionAvailability::Available,
                power_off: SessionActionAvailability::Available,
                lock: SessionActionAvailability::Available,
            },
            ..SessionSnapshot::default()
        };

        let rows = build_action_rows(&config, &snapshot);

        assert!(
            rows.iter()
                .all(|row| row.action != SessionAction::Hibernate)
        );
        assert!(rows.iter().any(|row| row.action == SessionAction::Logout));
    }
}
