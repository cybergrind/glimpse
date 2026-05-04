#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::{
        action_menu::{
            ActionMenu, ActionMenuItem, Init as ActionMenuInit, Input as ActionMenuInput,
        },
        animated_popover::AnimatedPopover,
        hero::HeroView,
        popover_shell::PopoverShell,
    },
    services::session::{
        SessionAction, SessionActionAvailability, SessionBackendState, SessionServiceHealth, State,
    },
};

use super::{Config, format};

pub struct Popover {
    animation: AnimatedPopover,
    config: Config,
    state: State,
    hero_icon_name: &'static str,
    hero_subtitle: String,
    session_actions_visible: bool,
    power_actions_visible: bool,
    session_actions: Controller<ActionMenu<SessionAction>>,
    power_actions: Controller<ActionMenu<SessionAction>>,
}

pub struct Init {
    pub parent: gtk::Box,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    Toggle,
    Close,
    UpdateState(State),
    Reconfigure(Config),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    Opened,
    Closed,
    ActionRequested(SessionAction),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        root = gtk::Popover {
            add_css_class: "session-popover",
            add_css_class: "popover-size-small",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[name = "hero"]
                    #[template]
                    HeroView,

                    #[name = "hero_separator"]
                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[local_ref]
                    session_actions_widget -> gtk::Box {},

                    #[name = "action_separator"]
                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[local_ref]
                    power_actions_widget -> gtk::Box {},
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let state = State::default();
        let session_actions = ActionMenu::builder()
            .launch(ActionMenuInit {
                header: Some("Session".into()),
                items: build_session_items(&init.config, &state),
            })
            .forward(sender.output_sender(), Output::ActionRequested);
        let power_actions = ActionMenu::builder()
            .launch(ActionMenuInit {
                header: Some("Power".into()),
                items: build_power_items(&init.config, &state),
            })
            .forward(sender.output_sender(), Output::ActionRequested);
        let session_actions_widget = session_actions.widget().clone();
        let power_actions_widget = power_actions.widget().clone();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(Output::Opened);
        });

        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(Output::Closed);
        });

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            config: init.config,
            state,
            hero_icon_name: "avatar-default-symbolic",
            hero_subtitle: String::new(),
            session_actions_visible: false,
            power_actions_visible: false,
            session_actions,
            power_actions,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Toggle => self.animation.toggle(),
            Input::Close => self.animation.close(),
            Input::UpdateState(state) => {
                self.hero_icon_name = hero_icon_name(&state);
                self.hero_subtitle = hero_subtitle(&state);
                self.state = state;
                self.update_actions();
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.update_actions();
            }
        }
    }

    fn post_view() {
        hero.icon.set_icon_name(Some(model.hero_icon_name));
        hero.title.set_label(&model.state.snapshot.user_name);
        hero.subtitle.set_label(&model.hero_subtitle);

        hero_separator.set_visible(model.session_actions_visible || model.power_actions_visible);
        action_separator.set_visible(model.session_actions_visible && model.power_actions_visible);
    }
}

impl Popover {
    fn update_actions(&mut self) {
        let session_items = build_session_items(&self.config, &self.state);
        let power_items = build_power_items(&self.config, &self.state);

        self.session_actions_visible = has_visible_items(&session_items);
        self.power_actions_visible = has_visible_items(&power_items);
        self.session_actions
            .emit(ActionMenuInput::Update(session_items));
        self.power_actions
            .emit(ActionMenuInput::Update(power_items));
    }
}

fn has_visible_items(items: &[ActionMenuItem<SessionAction>]) -> bool {
    items.iter().any(|item| item.visible)
}

fn build_session_items(config: &Config, state: &State) -> Vec<ActionMenuItem<SessionAction>> {
    vec![
        action_item(
            "Lock Screen",
            "system-lock-screen-symbolic",
            config.show_lock && action_available(&state.snapshot.capabilities.lock),
            SessionAction::Lock,
        ),
        action_item(
            "Log Out",
            "system-log-out-symbolic",
            config.show_logout
                && matches!(
                    state.snapshot.capabilities.backend,
                    SessionBackendState::Available
                ),
            SessionAction::Logout,
        ),
    ]
}

fn build_power_items(config: &Config, state: &State) -> Vec<ActionMenuItem<SessionAction>> {
    let capabilities = &state.snapshot.capabilities;
    vec![
        action_item(
            "Suspend",
            "media-playback-pause-symbolic",
            config.show_suspend && action_available(&capabilities.suspend),
            SessionAction::Suspend,
        ),
        action_item(
            "Hibernate",
            "document-save-symbolic",
            config.show_hibernate && action_available(&capabilities.hibernate),
            SessionAction::Hibernate,
        ),
        action_item(
            "Restart",
            "system-reboot-symbolic",
            config.show_reboot && action_available(&capabilities.reboot),
            SessionAction::Reboot,
        ),
        action_item(
            "Shut Down",
            "system-shutdown-symbolic",
            config.show_shutdown && action_available(&capabilities.power_off),
            SessionAction::PowerOff,
        ),
    ]
}

fn action_item(
    label: &str,
    icon: &str,
    visible: bool,
    command: SessionAction,
) -> ActionMenuItem<SessionAction> {
    ActionMenuItem {
        label: label.into(),
        icon: Some(icon.into()),
        visible,
        checked: None,
        selectable: None,
        command,
    }
}

fn action_available(availability: &SessionActionAvailability) -> bool {
    matches!(
        availability,
        SessionActionAvailability::Available | SessionActionAvailability::Challenge
    )
}

fn hero_icon_name(state: &State) -> &'static str {
    match state.active_action {
        Some(action) => action_icon_name(action),
        None => "avatar-default-symbolic",
    }
}

fn action_icon_name(action: SessionAction) -> &'static str {
    match action {
        SessionAction::Lock => "system-lock-screen-symbolic",
        SessionAction::Logout => "system-log-out-symbolic",
        SessionAction::Suspend => "media-playback-pause-symbolic",
        SessionAction::Hibernate => "document-save-symbolic",
        SessionAction::Reboot => "system-reboot-symbolic",
        SessionAction::PowerOff => "system-shutdown-symbolic",
    }
}

fn hero_subtitle(state: &State) -> String {
    match &state.health {
        SessionServiceHealth::Degraded { message } => return message.clone(),
        SessionServiceHealth::Ready => {}
    }

    if state.active_action.is_some() {
        format::state_text(state)
    } else if state.snapshot.subtitle.is_empty() {
        state.snapshot.host_name.clone()
    } else {
        state.snapshot.subtitle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::session::{SessionActionCapabilities, SessionSnapshot};

    #[test]
    fn action_available_accepts_challenge() {
        assert!(action_available(&SessionActionAvailability::Available));
        assert!(action_available(&SessionActionAvailability::Challenge));
        assert!(!action_available(&SessionActionAvailability::Unavailable));
    }

    #[test]
    fn builds_visible_items_from_config_and_capabilities() {
        let state = State {
            snapshot: SessionSnapshot {
                capabilities: SessionActionCapabilities {
                    backend: SessionBackendState::Available,
                    lock: SessionActionAvailability::Available,
                    suspend: SessionActionAvailability::Challenge,
                    hibernate: SessionActionAvailability::Available,
                    reboot: SessionActionAvailability::Unavailable,
                    power_off: SessionActionAvailability::Available,
                },
                ..SessionSnapshot::default()
            },
            ..State::default()
        };
        let config = Config {
            show_hibernate: true,
            ..Config::default()
        };

        let session_items = build_session_items(&config, &state);
        let power_items = build_power_items(&config, &state);

        assert!(
            session_items
                .iter()
                .any(|item| item.command == SessionAction::Lock && item.visible)
        );
        assert!(
            power_items
                .iter()
                .any(|item| item.command == SessionAction::Suspend && item.visible)
        );
        assert!(
            power_items
                .iter()
                .any(|item| item.command == SessionAction::Hibernate && item.visible)
        );
        assert!(
            power_items
                .iter()
                .any(|item| item.command == SessionAction::PowerOff && item.visible)
        );
        assert!(
            power_items
                .iter()
                .any(|item| item.command == SessionAction::Reboot && !item.visible)
        );
    }

    #[test]
    fn visible_item_helper_tracks_section_visibility() {
        let state = State::default();
        let items = build_session_items(&Config::default(), &state);

        assert!(!has_visible_items(&items));

        let state = State {
            snapshot: SessionSnapshot {
                capabilities: SessionActionCapabilities {
                    backend: SessionBackendState::Unavailable,
                    lock: SessionActionAvailability::Available,
                    suspend: SessionActionAvailability::Unavailable,
                    hibernate: SessionActionAvailability::Unavailable,
                    reboot: SessionActionAvailability::Unavailable,
                    power_off: SessionActionAvailability::Unavailable,
                },
                ..SessionSnapshot::default()
            },
            ..State::default()
        };
        let items = build_session_items(&Config::default(), &state);

        assert!(has_visible_items(&items));
    }
}
