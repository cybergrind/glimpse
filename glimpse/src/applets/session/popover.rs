#![allow(unused_assignments)]

use glimpse::session_actions::provider::SessionSnapshot;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::{
    SessionConfig,
    components::{
        action_list::{SessionActionList, SessionActionListInput, SessionActionListOutput},
        hero::{SessionHero, SessionHeroInput, SessionHeroView},
    },
};

pub struct SessionPopover {
    popover: gtk::Popover,
    #[allow(dead_code)]
    hero: Controller<SessionHero>,
    #[allow(dead_code)]
    actions: Controller<SessionActionList>,
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

#[derive(Debug)]
pub enum SessionPopoverInput {
    Toggle,
    Update(SessionSnapshot),
    Reconfigure {
        config: SessionConfig,
        snapshot: SessionSnapshot,
    },
    Close,
}

#[derive(Debug, Clone)]
pub enum SessionPopoverOutput {
    ActionRequested(SessionAction),
}

#[relm4::component(pub)]
impl SimpleComponent for SessionPopover {
    type Init = SessionPopoverInit;
    type Input = SessionPopoverInput;
    type Output = SessionPopoverOutput;

    view! {
        gtk::Popover {
            add_css_class: "session-popover",
            set_hexpand: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_overflow: gtk::Overflow::Hidden,

                #[local_ref]
                hero_widget -> gtk::Box {},

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[local_ref]
                actions_widget -> gtk::Box {},
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let hero = SessionHero::builder()
            .launch(SessionHeroView::default())
            .detach();
        let actions = SessionActionList::builder().launch(init.config).forward(
            sender.output_sender(),
            |output| match output {
                SessionActionListOutput::ActionRequested(action) => {
                    SessionPopoverOutput::ActionRequested(action)
                }
            },
        );

        let hero_widget = hero.widget().clone();
        let actions_widget = actions.widget().clone();
        let model = SessionPopover {
            popover: root.clone(),
            hero,
            actions,
        };
        let widgets = view_output!();

        model.popover.set_parent(&init.parent);
        model.popover.set_autohide(true);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            SessionPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            SessionPopoverInput::Update(snapshot) => {
                self.hero
                    .emit(SessionHeroInput::Update(SessionHeroView::from(&snapshot)));
                self.actions.emit(SessionActionListInput::Update(snapshot));
            }
            SessionPopoverInput::Reconfigure { config, snapshot } => {
                self.actions
                    .emit(SessionActionListInput::Reconfigure { config, snapshot });
            }
            SessionPopoverInput::Close => self.popover.popdown(),
        }
    }
}

#[cfg(test)]
mod tests {
    use glimpse::session_actions::provider::{
        SessionActionAvailability, SessionActionCapabilities, SessionBackendState, SessionSnapshot,
    };

    use super::super::{SessionConfig, components::action_list::build_action_rows};
    use super::*;

    #[test]
    fn build_action_rows_inserts_a_single_separator_before_power_actions() {
        let config = SessionConfig {
            show_lock: true,
            show_logout: true,
            show_suspend: true,
            show_hibernate: true,
            show_reboot: true,
            show_shutdown: true,
            ..SessionConfig::default()
        };
        let snapshot = SessionSnapshot {
            capabilities: SessionActionCapabilities {
                backend: SessionBackendState::Available,
                lock: SessionActionAvailability::Available,
                suspend: SessionActionAvailability::Available,
                hibernate: SessionActionAvailability::Available,
                reboot: SessionActionAvailability::Available,
                power_off: SessionActionAvailability::Available,
            },
            ..SessionSnapshot::default()
        };

        let rows = build_action_rows(&config, &snapshot);

        assert_eq!(rows.len(), 6);
        assert!(!rows[0].separated);
        assert!(!rows[1].separated);
        assert!(rows[2].separated);
        assert_eq!(rows[2].action, SessionAction::Suspend);
        assert!(!rows[3].separated);
        assert!(!rows[4].separated);
        assert!(!rows[5].separated);
    }

    #[test]
    fn build_action_rows_respects_backend_and_capability_availability() {
        let config = SessionConfig::default();
        let snapshot = SessionSnapshot {
            capabilities: SessionActionCapabilities {
                backend: SessionBackendState::Unavailable,
                lock: SessionActionAvailability::Unavailable,
                suspend: SessionActionAvailability::Challenge,
                hibernate: SessionActionAvailability::Unavailable,
                reboot: SessionActionAvailability::Available,
                power_off: SessionActionAvailability::Unavailable,
            },
            ..SessionSnapshot::default()
        };

        let rows = build_action_rows(&config, &snapshot);

        let lock = rows
            .iter()
            .find(|row| row.action == SessionAction::Lock)
            .unwrap();
        let logout = rows
            .iter()
            .find(|row| row.action == SessionAction::Logout)
            .unwrap();
        let suspend = rows
            .iter()
            .find(|row| row.action == SessionAction::Suspend)
            .unwrap();
        let reboot = rows
            .iter()
            .find(|row| row.action == SessionAction::Reboot)
            .unwrap();

        assert!(!lock.enabled);
        assert!(!logout.enabled);
        assert!(suspend.enabled);
        assert!(reboot.enabled);
    }
}
