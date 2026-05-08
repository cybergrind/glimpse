use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, gio, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        clipboard::{ClipboardHandle, Command, State},
        framework::ServiceCommand,
    },
};

use super::{
    format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(alias = "label")]
    pub label_format: String,
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
    pub show_when_empty: bool,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid clipboard applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            label_format: format::DEFAULT_LABEL_FORMAT.into(),
            tooltip_format: format::DEFAULT_TOOLTIP_FORMAT.into(),
            show_when_empty: false,
        }
    }
}

pub struct Applet {
    config: Config,
    state: State,
    icon_name: String,
    label: String,
    tooltip: String,
    service: ClipboardHandle,
    popover: Controller<Popover>,
    action_popover: gtk::PopoverMenu,
    refresh_action: gio::SimpleAction,
    clear_history_action: gio::SimpleAction,
    clear_clipboard_action: gio::SimpleAction,
    popover_open: bool,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: ClipboardHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    TogglePopover,
    OpenActions,
    Refresh,
    ClearHistory,
    ClearClipboard,
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
            add_css_class: "hoverable",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            #[watch]
            set_visible: model.visible(),
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
                add_css_class: "clipboard-label",
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
        let (action_popover, refresh_action, clear_history_action, clear_clipboard_action) =
            build_context_menu(&root, &sender);
        sync_action_sensitivity(
            &state,
            &refresh_action,
            &clear_history_action,
            &clear_clipboard_action,
        );
        let model = Applet {
            icon_name: format::icon_name(&state).into(),
            label: format::label(&init.config.label_format, &state),
            tooltip: format::tooltip(&init.config.tooltip_format, &state),
            config: init.config,
            state,
            service: init.service,
            popover,
            action_popover,
            refresh_action,
            clear_history_action,
            clear_clipboard_action,
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

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                self.state = state;
                self.sync();
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.sync();
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::OpenActions => {
                self.sync_action_menu();
                self.action_popover.popup();
            }
            Input::Refresh => {
                self.action_popover.popdown();
                self.send_command(Command::Refresh);
            }
            Input::ClearHistory => {
                self.action_popover.popdown();
                self.send_command(Command::ClearHistory);
            }
            Input::ClearClipboard => {
                self.action_popover.popdown();
                self.send_command(Command::ClearClipboard);
            }
            Input::PopoverOutput(PopoverOutput::Opened) => {
                self.popover_open = true;
                self.sync_popover_state();
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
    fn visible(&self) -> bool {
        self.state.available && (self.config.show_when_empty || !self.state.history.is_empty())
    }

    fn sync(&mut self) {
        self.icon_name = format::icon_name(&self.state).into();
        self.label = format::label(&self.config.label_format, &self.state);
        self.tooltip = format::tooltip(&self.config.tooltip_format, &self.state);
        if self.popover_open {
            self.sync_popover_state();
        }
        self.sync_action_menu();
    }

    fn sync_popover_state(&self) {
        self.popover
            .emit(PopoverInput::UpdateState(self.state.clone()));
    }

    fn send_command(&self, command: Command) {
        if let Err(error) = self.service.try_send(ServiceCommand::Command(command)) {
            tracing::warn!(%error, "failed to send clipboard command");
        }
    }

    fn sync_action_menu(&self) {
        sync_action_sensitivity(
            &self.state,
            &self.refresh_action,
            &self.clear_history_action,
            &self.clear_clipboard_action,
        );
    }
}

fn clear_history_action_available(state: &State) -> bool {
    state.available && !state.history.is_empty()
}

fn clear_clipboard_action_available(state: &State) -> bool {
    state.available && state.current_id.is_some()
}

fn sync_action_sensitivity(
    state: &State,
    refresh_action: &gio::SimpleAction,
    clear_history_action: &gio::SimpleAction,
    clear_clipboard_action: &gio::SimpleAction,
) {
    refresh_action.set_enabled(true);
    clear_history_action.set_enabled(clear_history_action_available(state));
    clear_clipboard_action.set_enabled(clear_clipboard_action_available(state));
}

fn build_context_menu(
    root: &gtk::Box,
    sender: &ComponentSender<Applet>,
) -> (
    gtk::PopoverMenu,
    gio::SimpleAction,
    gio::SimpleAction,
    gio::SimpleAction,
) {
    let action_group = gio::SimpleActionGroup::new();

    let refresh_action = gio::SimpleAction::new("refresh", None);
    refresh_action.connect_activate({
        let sender = sender.input_sender().clone();
        move |_, _| sender.emit(Input::Refresh)
    });
    action_group.add_action(&refresh_action);

    let clear_history_action = gio::SimpleAction::new("clear-history", None);
    clear_history_action.connect_activate({
        let sender = sender.input_sender().clone();
        move |_, _| sender.emit(Input::ClearHistory)
    });
    action_group.add_action(&clear_history_action);

    let clear_clipboard_action = gio::SimpleAction::new("clear-clipboard", None);
    clear_clipboard_action.connect_activate({
        let sender = sender.input_sender().clone();
        move |_, _| sender.emit(Input::ClearClipboard)
    });
    action_group.add_action(&clear_clipboard_action);

    root.insert_action_group("clipboard", Some(&action_group));

    let menu = gio::Menu::new();
    menu.append(Some("Refresh"), Some("clipboard.refresh"));
    menu.append(Some("Clear History"), Some("clipboard.clear-history"));
    menu.append(Some("Clear Clipboard"), Some("clipboard.clear-clipboard"));

    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(root);
    popover.set_has_arrow(false);
    root.connect_destroy({
        let popover = popover.clone();
        move |_| popover.unparent()
    });

    (
        popover,
        refresh_action,
        clear_history_action,
        clear_clipboard_action,
    )
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_to_empty_label_and_hidden_empty_state() {
        let config = Config::default();

        assert_eq!(config.label_format, "");
        assert!(!config.show_when_empty);
    }

    #[test]
    fn clear_action_requires_available_history() {
        let mut state = State::default();
        assert!(!clear_history_action_available(&state));

        state.available = true;
        assert!(!clear_history_action_available(&state));
    }

    #[test]
    fn clear_clipboard_action_requires_current_entry() {
        let mut state = State::default();
        assert!(!clear_clipboard_action_available(&state));

        state.available = true;
        state.current_id = Some(7);
        assert!(clear_clipboard_action_available(&state));
    }
}
