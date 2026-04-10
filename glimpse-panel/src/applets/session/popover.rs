use glimpse::providers::session_actions::{
    SessionActionAvailability, SessionBackendState, SessionSnapshot,
};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::config::SessionConfig;

pub struct SessionPopover {
    popover: gtk::Popover,
    config: SessionConfig,
    name_label: gtk::Label,
    subtitle_label: gtk::Label,
    actions_box: gtk::Box,
}

pub struct SessionPopoverInit {
    pub parent: gtk::Box,
    pub config: SessionConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Lock,
    Logout,
    Suspend,
    Hibernate,
    Reboot,
    PowerOff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionRow {
    action: SessionAction,
    icon_name: &'static str,
    label: &'static str,
    enabled: bool,
    separated: bool,
}

#[derive(Debug)]
pub enum SessionPopoverInput {
    Toggle,
    Update(SessionSnapshot),
    Close,
}

#[derive(Debug, Clone)]
pub enum SessionPopoverOutput {
    ActionRequested(SessionAction),
}

impl SimpleComponent for SessionPopover {
    type Init = SessionPopoverInit;
    type Input = SessionPopoverInput;
    type Output = SessionPopoverOutput;
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Popover::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("session-popover");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hero.add_css_class("session-hero");

        let avatar = gtk::Image::from_icon_name("avatar-default-symbolic");
        avatar.set_pixel_size(32);
        hero.append(&avatar);

        let info_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        info_box.set_hexpand(true);
        info_box.set_valign(gtk::Align::Center);

        let name_label = gtk::Label::new(Some("user"));
        name_label.set_halign(gtk::Align::Start);
        name_label.add_css_class("session-hero-name");
        info_box.append(&name_label);

        let subtitle_label = gtk::Label::new(None);
        subtitle_label.set_halign(gtk::Align::Start);
        subtitle_label.add_css_class("session-hero-subtitle");
        info_box.append(&subtitle_label);

        hero.append(&info_box);
        vbox.append(&hero);

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let actions_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.append(&actions_box);

        root.set_child(Some(&vbox));

        let model = SessionPopover {
            popover: root.clone(),
            config: init.config,
            name_label,
            subtitle_label,
            actions_box,
        };
        model.render_rows(&sender, &SessionSnapshot::default());

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            SessionPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            SessionPopoverInput::Update(snapshot) => {
                self.name_label.set_label(&snapshot.user_name);
                self.subtitle_label.set_label(&snapshot.subtitle);
                self.render_rows(&sender, &snapshot);
            }
            SessionPopoverInput::Close => self.popover.popdown(),
        }
    }
}

impl SessionPopover {
    fn render_rows(&self, sender: &ComponentSender<Self>, snapshot: &SessionSnapshot) {
        while let Some(child) = self.actions_box.first_child() {
            self.actions_box.remove(&child);
        }

        for row in build_rows(&self.config, snapshot) {
            if row.separated {
                self.actions_box
                    .append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            }
            self.actions_box.append(&build_action_row(row, sender));
        }
    }
}

fn build_rows(config: &SessionConfig, snapshot: &SessionSnapshot) -> Vec<SessionRow> {
    let caps = &snapshot.capabilities;
    let mut rows = Vec::new();

    if config.show_lock {
        rows.push(SessionRow {
            action: SessionAction::Lock,
            icon_name: "system-lock-screen-symbolic",
            label: "Lock Screen",
            enabled: action_enabled(&caps.lock),
            separated: false,
        });
    }

    if config.show_logout {
        rows.push(SessionRow {
            action: SessionAction::Logout,
            icon_name: "system-log-out-symbolic",
            label: "Log Out",
            enabled: matches!(caps.backend, SessionBackendState::Available),
            separated: false,
        });
    }

    let has_session_rows = !rows.is_empty();

    if config.show_suspend {
        rows.push(SessionRow {
            action: SessionAction::Suspend,
            icon_name: "media-playback-pause-symbolic",
            label: "Suspend",
            enabled: action_enabled(&caps.suspend),
            separated: has_session_rows,
        });
    }

    if config.show_hibernate {
        rows.push(SessionRow {
            action: SessionAction::Hibernate,
            icon_name: "document-save-symbolic",
            label: "Hibernate",
            enabled: action_enabled(&caps.hibernate),
            separated: has_session_rows && !rows.iter().any(|row| row.separated),
        });
    }

    if config.show_reboot {
        rows.push(SessionRow {
            action: SessionAction::Reboot,
            icon_name: "system-reboot-symbolic",
            label: "Restart",
            enabled: action_enabled(&caps.reboot),
            separated: has_session_rows && !rows.iter().any(|row| row.separated),
        });
    }

    if config.show_shutdown {
        rows.push(SessionRow {
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

fn build_action_row(row: SessionRow, sender: &ComponentSender<SessionPopover>) -> gtk::Button {
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
        let _ = sender.output(SessionPopoverOutput::ActionRequested(action));
    });

    button
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::providers::session_actions::{
        SessionActionCapabilities, SessionSnapshot,
    };

    #[test]
    fn session_rows_respect_config_visibility() {
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
            user_name: "alex".into(),
            host_name: "workstation".into(),
            subtitle: "workstation · up 3h 2m".into(),
        };

        let rows = build_rows(&config, &snapshot);
        assert!(rows.iter().all(|row| row.action != SessionAction::Hibernate));
        assert!(rows.iter().any(|row| row.action == SessionAction::Logout));
    }

    #[test]
    fn unavailable_capabilities_disable_rows() {
        let rows = build_rows(
            &SessionConfig::default(),
            &SessionSnapshot {
                capabilities: SessionActionCapabilities::default(),
                ..SessionSnapshot::default()
            },
        );

        assert!(rows.iter().all(|row| !row.enabled));
    }
}
