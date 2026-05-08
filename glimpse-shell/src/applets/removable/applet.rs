use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, gio, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        framework::ServiceCommand,
        storage::{Command, State, StorageHandle},
    },
};

use super::{
    format,
    popover::{self, Popover, PopoverInit, PopoverInput, PopoverOutput},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    show_when_empty: bool,
    label_format: String,
    tooltip_format: String,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid removable applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_when_empty: false,
            label_format: String::new(),
            tooltip_format: "{count} removable device(s), {mounted} mounted".into(),
        }
    }
}

pub struct Applet {
    visible: bool,
    config: Config,
    tooltip: String,
    label: String,
    icon_name: String,
    state: State,
    service: StorageHandle,
    popover: Controller<Popover>,
    action_popover: gtk::PopoverMenu,
    action_group: gio::SimpleActionGroup,
    popover_open: bool,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: StorageHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    TogglePopover,
    OpenActions,
    MenuCommand(Command),
    PopoverOutput(PopoverOutput),
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            add_css_class: "removable",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            #[watch]
            set_visible: model.visible,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::TogglePopover);
                },
            },
            add_controller = gtk::GestureClick {
                set_button: 3,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(Input::OpenActions);
                },
            },

            gtk::Image {
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                #[watch]
                set_icon_name: Some(&model.icon_name),
            },

            gtk::Label {
                set_valign: gtk::Align::Center,
                #[watch]
                set_label: &model.label,
                #[watch]
                set_visible: !model.label.is_empty(),
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = Popover::builder()
            .launch(PopoverInit {
                parent: root.clone(),
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let state = init.service.snapshot();
        let action_group = gio::SimpleActionGroup::new();
        root.insert_action_group("removable", Some(&action_group));
        let action_popover = gtk::PopoverMenu::from_model(Some(&gio::Menu::new()));
        action_popover.set_parent(&root);
        action_popover.set_has_arrow(false);
        root.connect_destroy({
            let action_popover = action_popover.clone();
            move |_| action_popover.unparent()
        });

        let config = init.config;
        let model = Applet {
            visible: applet_visible(&config, &state),
            icon_name: icon_name_for_state(&state),
            label: format::label(&config.label_format, &state),
            tooltip: format::tooltip(&config.tooltip_format, &state),
            config,
            state,
            service: init.service,
            popover,
            action_popover,
            action_group,
            popover_open: false,
            subscription_cancel: CancellationToken::new(),
        };

        let service = model.service.clone();
        let cancel = model.subscription_cancel.clone();
        let subscription_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            if subscription_sender
                .send(Input::ServiceStateChanged(sub.borrow().clone()))
                .is_err()
            {
                return;
            }

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() {
                            break;
                        }

                        if subscription_sender
                            .send(Input::ServiceStateChanged(sub.borrow().clone()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                self.apply_state(state);
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.apply_state(self.service.snapshot());
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::OpenActions => {
                self.sync_action_menu(&sender);
                self.action_popover.popup();
            }
            Input::MenuCommand(command) => {
                self.action_popover.popdown();
                self.send_command(command);
            }
            Input::PopoverOutput(PopoverOutput::Opened) => {
                self.popover_open = true;
                self.sync_popover();
                self.send_command(Command::Refresh);
            }
            Input::PopoverOutput(PopoverOutput::Closed) => {
                self.popover_open = false;
            }
            Input::PopoverOutput(PopoverOutput::Command(command)) => {
                self.send_command(command);
            }
        }
    }
}

impl Applet {
    fn apply_state(&mut self, state: State) {
        self.visible = applet_visible(&self.config, &state);
        self.icon_name = icon_name_for_state(&state);
        self.label = format::label(&self.config.label_format, &state);
        self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
        self.state = state.clone();
        if self.popover_open {
            self.popover.emit(PopoverInput::UpdateState(state));
        }
    }

    fn sync_popover(&self) {
        self.popover
            .emit(PopoverInput::UpdateState(self.state.clone()));
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send storage command");
            }
        });
    }

    fn sync_action_menu(&self, sender: &ComponentSender<Self>) {
        for action in self.action_group.list_actions() {
            self.action_group.remove_action(action.as_str());
        }

        let menu = removable_context_menu(&self.state);
        for (index, command) in removable_context_commands(&self.state)
            .into_iter()
            .enumerate()
        {
            let action = gio::SimpleAction::new(&format!("action{index}"), None);
            action.connect_activate({
                let sender = sender.input_sender().clone();
                move |_, _| sender.emit(Input::MenuCommand(command.clone()))
            });
            self.action_group.add_action(&action);
        }
        self.action_popover.set_menu_model(Some(&menu));
    }
}

fn removable_context_commands(state: &State) -> Vec<Command> {
    let mut commands = vec![Command::Refresh];
    for device in &state.devices {
        commands.extend(
            popover::device_actions(device)
                .into_iter()
                .filter(|action| action.visible)
                .map(|action| action.command),
        );
    }
    commands
}

fn removable_context_menu(state: &State) -> gio::Menu {
    let menu = gio::Menu::new();
    for (index, item) in removable_context_items(state).iter().enumerate() {
        menu.append(Some(&item.label), Some(&format!("removable.action{index}")));
    }
    menu
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemovableContextItem {
    label: String,
}

fn removable_context_items(state: &State) -> Vec<RemovableContextItem> {
    let mut items = vec![RemovableContextItem {
        label: "Refresh".into(),
    }];
    for device in &state.devices {
        for action in popover::device_actions(device)
            .into_iter()
            .filter(|action| action.visible)
        {
            items.push(RemovableContextItem {
                label: format!("{}: {}", device.name, action.label),
            });
        }
    }
    items
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn applet_visible(config: &Config, state: &State) -> bool {
    config.show_when_empty || !state.devices.is_empty()
}

fn icon_name_for_state(state: &State) -> String {
    state
        .devices
        .iter()
        .find_map(|device| (!device.icon.is_empty()).then(|| device.icon.clone()))
        .unwrap_or_else(|| "drive-removable-media-symbolic".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::storage::StorageDevice;

    #[test]
    fn default_config_hides_empty_applet() {
        assert!(!applet_visible(&Config::default(), &State::default()));
    }

    #[test]
    fn applet_is_visible_when_devices_exist() {
        let state = State {
            devices: vec![StorageDevice {
                id: "device".into(),
                name: "USB Drive".into(),
                ..StorageDevice::default()
            }],
            ..State::default()
        };

        assert!(applet_visible(&Config::default(), &state));
    }

    #[test]
    fn applet_icon_uses_device_icon_or_generic_fallback() {
        assert_eq!(
            icon_name_for_state(&State::default()),
            "drive-removable-media-symbolic"
        );

        let state = State {
            devices: vec![StorageDevice {
                icon: "media-flash-sd-mmc-symbolic".into(),
                ..StorageDevice::default()
            }],
            ..State::default()
        };

        assert_eq!(icon_name_for_state(&state), "media-flash-sd-mmc-symbolic");
    }

    #[test]
    fn context_menu_includes_refresh_and_device_actions() {
        let state = State {
            devices: vec![StorageDevice {
                id: "device".into(),
                name: "USB Drive".into(),
                can_mount: true,
                can_eject: true,
                can_power_off: true,
                ..StorageDevice::default()
            }],
            ..State::default()
        };

        assert_eq!(
            removable_context_items(&state),
            vec![
                RemovableContextItem {
                    label: "Refresh".into(),
                },
                RemovableContextItem {
                    label: "USB Drive: Mount".into(),
                },
                RemovableContextItem {
                    label: "USB Drive: Eject".into(),
                },
                RemovableContextItem {
                    label: "USB Drive: Power Off".into(),
                },
            ]
        );
        assert_eq!(
            removable_context_commands(&state),
            vec![
                Command::Refresh,
                Command::Mount {
                    id: "device".into(),
                },
                Command::Eject {
                    id: "device".into(),
                },
                Command::PowerOff {
                    id: "device".into(),
                },
            ]
        );
    }
}
