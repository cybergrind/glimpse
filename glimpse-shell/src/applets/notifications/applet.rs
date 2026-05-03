use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    compositors::Window,
    panels::applets::AppletConfig,
    services::{
        compositor::{Command as CompositorCommand, CompositorHandle, State as CompositorState},
        framework::ServiceCommand,
        notifications::{
            NotificationsHandle,
            model::{Command, NotificationEntry, State},
        },
    },
};

use super::{
    activation, format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
    popup::{Popup, PopupInit, PopupInput, PopupPosition},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(alias = "label")]
    pub label_format: String,
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
    pub badge_style: String,
    pub popup_timeout_ms: u32,
    pub popup_visible_limit: usize,
    pub popup_position: PopupPosition,
    pub popup_margin_x: i32,
    pub popup_margin_y: i32,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "invalid notifications applet config, using defaults"
                );
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
            badge_style: "count".into(),
            popup_timeout_ms: 5000,
            popup_visible_limit: 8,
            popup_position: PopupPosition::TopCenter,
            popup_margin_x: 12,
            popup_margin_y: 32,
        }
    }
}

pub struct Applet {
    config: Config,
    state: State,
    compositor_state: CompositorState,
    service: NotificationsHandle,
    compositor: CompositorHandle,
    icon_name: String,
    label: String,
    tooltip: String,
    badge_label: String,
    badge_visible: bool,
    badge_classes: Vec<&'static str>,
    popover: Controller<Popover>,
    popup: Controller<Popup>,
    subscription_cancel: CancellationToken,
}

pub struct Init {
    pub service: NotificationsHandle,
    pub compositor: CompositorHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    CompositorStateChanged(CompositorState),
    Reconfigure(Config),
    TogglePopover,
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
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 3,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::TogglePopover);
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
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_valign: gtk::Align::Center,
                set_halign: gtk::Align::Center,
                #[watch]
                set_css_classes: &model.badge_classes,
                #[watch]
                set_visible: model.badge_visible,

                gtk::Label {
                    set_valign: gtk::Align::Center,
                    set_halign: gtk::Align::Center,
                    #[watch]
                    set_label: &model.badge_label,
                    #[watch]
                    set_visible: model.badge_style_uses_label(),
                }
            },
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
        let popup = Popup::builder()
            .launch(PopupInit {
                timeout_ms: init.config.popup_timeout_ms,
                visible_limit: init.config.popup_visible_limit,
                position: init.config.popup_position,
                margin_x: init.config.popup_margin_x,
                margin_y: init.config.popup_margin_y,
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let state = init.service.snapshot();
        let compositor_state = init.compositor.snapshot();
        let mut model = Applet {
            icon_name: format::icon_name(&state).into(),
            label: format::label(&init.config.label_format, &state),
            tooltip: format::tooltip(&init.config.tooltip_format, &state),
            badge_label: String::new(),
            badge_visible: false,
            badge_classes: Vec::new(),
            config: init.config,
            state,
            compositor_state,
            service: init.service,
            compositor: init.compositor,
            popover,
            popup,
            subscription_cancel: CancellationToken::new(),
        };
        model.apply_state(model.state.clone());

        subscribe_services(&model, sender.clone());

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => self.apply_state(state),
            Input::CompositorStateChanged(state) => self.compositor_state = state,
            Input::Reconfigure(config) => {
                self.config = config;
                self.apply_state(self.service.snapshot());
                self.popup.emit(PopupInput::Reconfigure {
                    timeout_ms: self.config.popup_timeout_ms,
                    visible_limit: self.config.popup_visible_limit,
                    position: self.config.popup_position,
                    margin_x: self.config.popup_margin_x,
                    margin_y: self.config.popup_margin_y,
                });
            }
            Input::TogglePopover => self.popover.emit(PopoverInput::Toggle),
            Input::PopoverOutput(output) => self.handle_output(output),
        }
    }
}

impl Applet {
    fn apply_state(&mut self, state: State) {
        self.icon_name = format::icon_name(&state).into();
        self.label = format::label(&self.config.label_format, &state);
        self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
        self.sync_badge(state.notifications.len(), state.dnd);
        self.popover.emit(PopoverInput::Update {
            notifications: state.notifications.clone(),
            dnd: state.dnd,
        });
        self.popup.emit(PopupInput::Update {
            notifications: state.notifications.clone(),
            dnd: state.dnd,
        });
        self.state = state;
    }

    fn sync_badge(&mut self, count: usize, dnd: bool) {
        self.badge_visible = count > 0 && !dnd && self.config.badge_style != "none";
        self.badge_label = if count > 9 {
            "9+".into()
        } else {
            count.to_string()
        };
        self.badge_classes = if self.config.badge_style == "count" {
            vec!["notification-badge-anchor", "badge", "is-accent"]
        } else {
            vec!["notification-badge-anchor", "status-dot", "is-accent"]
        };
    }

    fn badge_style_uses_label(&self) -> bool {
        self.config.badge_style == "count"
    }

    fn handle_output(&self, output: PopoverOutput) {
        match output {
            PopoverOutput::Dismiss(id) => self.send_notification(Command::Dismiss { id }),
            PopoverOutput::DismissMany(ids) => {
                for id in ids {
                    self.send_notification(Command::Dismiss { id });
                }
            }
            PopoverOutput::DismissAll => self.send_notification(Command::DismissAll),
            PopoverOutput::SetDnd(enabled) => self.send_notification(Command::SetDnd(enabled)),
            PopoverOutput::FocusAndDismiss(id) => self.focus_and_dismiss_notification(id),
            PopoverOutput::InvokeAction { id, action_key } => {
                self.invoke_action_and_dismiss(id, action_key);
            }
        }
    }

    fn focus_and_dismiss_notification(&self, id: u32) {
        let Some(notification) = self.state.notifications.iter().find(|item| item.id == id) else {
            tracing::debug!(id, "notification disappeared before focus and dismiss");
            self.send_notification(Command::Dismiss { id });
            return;
        };

        let focus_window = self.resolve_focus_window(notification);
        let compositor = self.compositor.clone();
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Some(window) = focus_window {
                send_compositor_command(&compositor, CompositorCommand::FocusWindow(window)).await;
            }
            send_notification_command(&service, Command::Dismiss { id }).await;
        });
    }

    fn invoke_action_and_dismiss(&self, id: u32, action_key: String) {
        let notification = self.state.notifications.iter().find(|item| item.id == id);
        let activation_token = notification.and_then(|notification| {
            activation::startup_notify_token(
                notification.desktop_entry.as_deref(),
                gtk::gdk::CURRENT_TIME,
            )
        });
        let mut focus_window = None;
        if activation_token.is_none() {
            if let Some(notification) = notification {
                focus_window = self.resolve_focus_window(notification);
            } else {
                tracing::debug!(id, "notification disappeared before action activation");
            }
        }

        let service = self.service.clone();
        let compositor = self.compositor.clone();
        relm4::spawn(async move {
            if let Some(window) = focus_window {
                send_compositor_command(&compositor, CompositorCommand::FocusWindow(window)).await;
            }
            let invoke = Command::InvokeAction {
                id,
                action_key,
                activation_token,
            };
            if send_notification_command(&service, invoke).await {
                send_notification_command(&service, Command::Dismiss { id }).await;
            }
        });
    }

    fn resolve_focus_window(&self, notification: &NotificationEntry) -> Option<usize> {
        let window = matching_window(&self.compositor_state, notification);
        if window.is_none() {
            tracing::debug!(
                id = notification.id,
                app_name = %notification.app_name,
                desktop_entry = ?notification.desktop_entry,
                "could not detect notification source window to focus"
            );
        }
        window
    }

    fn send_notification(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            send_notification_command(&service, command).await;
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn subscribe_services(model: &Applet, sender: ComponentSender<Applet>) {
    let service = model.service.clone();
    let compositor = model.compositor.clone();
    let cancel = model.subscription_cancel.clone();
    relm4::spawn(async move {
        let mut notifications = service.subscribe();
        let mut compositor_state = compositor.subscribe();
        sender.input(Input::ServiceStateChanged(notifications.borrow().clone()));
        sender.input(Input::CompositorStateChanged(
            compositor_state.borrow().clone(),
        ));

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = notifications.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    sender.input(Input::ServiceStateChanged(notifications.borrow().clone()));
                }
                changed = compositor_state.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    sender.input(Input::CompositorStateChanged(compositor_state.borrow().clone()));
                }
            }
        }
    });
}

async fn send_notification_command(service: &NotificationsHandle, command: Command) -> bool {
    match service.send(ServiceCommand::Command(command)).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "failed to send notifications command");
            false
        }
    }
}

async fn send_compositor_command(
    compositor: &CompositorHandle,
    command: CompositorCommand,
) -> bool {
    match compositor.send(ServiceCommand::Command(command)).await {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "failed to send compositor command");
            false
        }
    }
}

fn matching_window(state: &CompositorState, notification: &NotificationEntry) -> Option<usize> {
    let keys = notification_keys(notification);
    if keys.is_empty() {
        return None;
    }

    state
        .windows
        .iter()
        .find(|window| window_matches(window, &keys))
        .map(|window| window.id)
}

fn notification_keys(notification: &NotificationEntry) -> Vec<String> {
    [
        notification.desktop_entry.as_deref(),
        Some(&notification.app_name),
    ]
    .into_iter()
    .flatten()
    .filter_map(normalize_app_key)
    .collect()
}

fn normalize_app_key(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    Some(
        value
            .strip_suffix(".desktop")
            .unwrap_or(value)
            .to_ascii_lowercase(),
    )
}

fn window_matches(window: &Window, keys: &[String]) -> bool {
    let Some(app_id) = window.app_id.as_deref().and_then(normalize_app_key) else {
        return false;
    };

    keys.iter().any(|key| key == &app_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_matching_ignores_desktop_suffix_and_case() {
        let mut state = CompositorState::default();
        state.windows.push(Window {
            id: 7,
            title: None,
            app_id: Some("org.mozilla.firefox".into()),
            pid: None,
            layout_order: None,
            workspace: None,
            focused: false,
            urgent: false,
            fullscreen: false,
            floating: None,
        });
        let notification = NotificationEntry {
            id: 1,
            app_name: "Firefox".into(),
            app_icon: String::new(),
            desktop_entry: Some("Org.Mozilla.Firefox.desktop".into()),
            summary: String::new(),
            body: String::new(),
            urgency: 1,
            actions: Vec::new(),
            image: None,
            timestamp: 0,
            resident: false,
        };

        assert_eq!(matching_window(&state, &notification), Some(7));
    }

    #[test]
    fn config_accepts_bottom_right_popup_position() {
        let raw = AppletConfig {
            extends: None,
            settings: toml::toml! {
                popup_position = "bottom_right"
                popup_margin_x = 24
                popup_margin_y = 40
                popup_visible_limit = 6
            }
            .into(),
        };

        let config = Config::from_raw(&Some(raw));

        assert_eq!(config.popup_position, PopupPosition::BottomRight);
        assert_eq!(config.popup_margin_x, 24);
        assert_eq!(config.popup_margin_y, 40);
        assert_eq!(config.popup_visible_limit, 6);
    }

    #[test]
    fn config_defaults_popup_position_to_top_center() {
        assert_eq!(Config::default().popup_position, PopupPosition::TopCenter);
    }

    #[test]
    fn config_defaults_popup_visible_limit_to_eight() {
        assert_eq!(Config::default().popup_visible_limit, 8);
    }
}
