use std::rc::Rc;

use super::NotificationActionCommand;
use super::NotificationsConfig;
use super::popover::{NotificationsPopover, NotificationsPopoverInit, NotificationsPopoverInput};
use glimpse::notifications::{
    NotificationEntry, NotificationsServiceHandle, NotificationsServiceState,
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

pub struct Notifications {
    config: NotificationsConfig,
    icon_name: String,
    badge_label: String,
    badge_count: u32,
    badge_visible: bool,
    badge_style: String, // "count", "dot", ""
    tooltip: String,
    service: NotificationsServiceHandle,
    latest_state: Option<NotificationsServiceState>,
    dnd: bool,
    started_at: u64,
    popover: Controller<NotificationsPopover>,
    icon_widget: gtk::Image,
    badge_widget: gtk::Box,
    badge_value_label: gtk::Label,
}

pub struct NotificationsInit {
    pub config: NotificationsConfig,
    pub service: NotificationsServiceHandle,
}

#[derive(Debug)]
pub enum NotificationsMsg {
    ServiceState(NotificationsServiceState),
    Reconfigure(NotificationsConfig),
    TogglePopover,
    Command(NotificationActionCommand),
    Unavailable,
}

fn filter_fresh_notifications(
    notifications: &[NotificationEntry],
    started_at: u64,
) -> Vec<NotificationEntry> {
    notifications
        .iter()
        .filter(|notification| notification.timestamp >= started_at)
        .cloned()
        .collect()
}

fn notification_counts(notifications: &[NotificationEntry]) -> (u32, u32) {
    let count = notifications.len() as u32;
    let badge_count = count;
    (count, badge_count)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BadgePresentation {
    css_classes: &'static [&'static str],
    label: String,
    show_label: bool,
}

fn badge_presentation(style: &str, badge_count: u32) -> BadgePresentation {
    if style == "count" {
        BadgePresentation {
            css_classes: &["badge", "is-accent"],
            label: if badge_count > 9 {
                "9+".into()
            } else {
                badge_count.to_string()
            },
            show_label: true,
        }
    } else {
        BadgePresentation {
            css_classes: &["status-dot", "is-accent"],
            label: String::new(),
            show_label: false,
        }
    }
}

fn tooltip_text(dnd: bool, count: u32) -> String {
    if dnd {
        "Do Not Disturb".into()
    } else if count > 0 {
        format!("{count} notification{}", if count > 1 { "s" } else { "" })
    } else {
        "Notifications".into()
    }
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl Component for Notifications {
    type Init = NotificationsInit;
    type Input = NotificationsMsg;
    type Output = ();
    type CommandOutput = NotificationsMsg;

    view! {
        gtk::Box {
            set_spacing: 3,
            add_css_class: "applet",
            add_css_class: "notifications",

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(NotificationsMsg::TogglePopover);
                }
            },

            #[name = "indicator_box"]
            gtk::Box {
                set_spacing: 3,
                set_valign: gtk::Align::Center,

                #[name = "icon_widget"]
                gtk::Image {
                    set_pixel_size: 16,
                    set_valign: gtk::Align::Center,
                },

                #[name = "badge_widget"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_valign: gtk::Align::Center,
                    set_halign: gtk::Align::Center,
                    add_css_class: "notification-badge-anchor",

                    #[name = "badge_value_label"]
                    gtk::Label {
                        set_valign: gtk::Align::Center,
                        set_halign: gtk::Align::Center,
                    }
                }
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        tracing::info!("notifications applet: initializing");
        let emit_command = Rc::new({
            let sender = sender.clone();
            move |command| sender.input(NotificationsMsg::Command(command))
        });
        let popover = NotificationsPopover::builder()
            .launch(NotificationsPopoverInit {
                parent: root.clone(),
                emit_command: emit_command.clone(),
            })
            .detach();

        let widgets = view_output!();

        let model = Notifications {
            config: init.config.clone(),
            icon_name: "preferences-system-notifications-symbolic".into(),
            badge_label: String::new(),
            badge_count: 0,
            badge_visible: false,
            badge_style: init.config.badge_style.clone(),
            tooltip: "Notifications".into(),
            service: init.service.clone(),
            latest_state: None,
            dnd: false,
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            popover,
            icon_widget: widgets.icon_widget.clone(),
            badge_widget: widgets.badge_widget.clone(),
            badge_value_label: widgets.badge_value_label.clone(),
        };
        model.refresh_indicator_widgets(&root);

        let service = init.service;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("notifications applet: subscribing to shared service");
                    let mut state_rx = service.subscribe();
                    let _ = out.send(NotificationsMsg::ServiceState(state_rx.borrow().clone()));

                    while state_rx.changed().await.is_ok() {
                        let _ = out.send(NotificationsMsg::ServiceState(state_rx.borrow().clone()));
                    }

                    let _ = out.send(NotificationsMsg::Unavailable);
                })
                .drop_on_shutdown()
        });

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

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            NotificationsMsg::ServiceState(state) => {
                self.latest_state = Some(state.clone());
                let dnd = state.dnd;
                self.dnd = dnd;
                self.icon_name = if dnd {
                    "notifications-disabled-symbolic"
                } else {
                    "preferences-system-notifications-symbolic"
                }
                .into();

                let fresh = filter_fresh_notifications(&state.notifications, self.started_at);
                let (count, badge_count) = notification_counts(&fresh);

                self.badge_visible = badge_count > 0 && !self.badge_style.is_empty();
                self.badge_count = badge_count;
                self.badge_label = badge_presentation(&self.badge_style, badge_count).label;
                self.tooltip = tooltip_text(self.dnd, count);
                self.refresh_indicator_widgets(_root);

                self.popover.emit(NotificationsPopoverInput::UpdateStatus {
                    dnd: self.dnd,
                    count,
                });
                self.popover
                    .emit(NotificationsPopoverInput::UpdateList(fresh));
            }
            NotificationsMsg::Reconfigure(config) => {
                self.config = config.clone();
                self.badge_style = config.badge_style;
                if let Some(state) = self.latest_state.clone() {
                    _sender.input(NotificationsMsg::ServiceState(state));
                } else {
                    self.badge_visible = false;
                    self.badge_count = 0;
                    self.badge_label.clear();
                    self.refresh_indicator_widgets(_root);
                }
            }
            NotificationsMsg::TogglePopover => {
                self.popover.emit(NotificationsPopoverInput::Toggle);
            }
            NotificationsMsg::Command(command) => {
                let service = self.service.clone();
                relm4::spawn(async move {
                    if let Err(error) = service.send(command.into_service_command()).await {
                        tracing::warn!(error = %error, "notifications applet: command failed");
                    }
                });
            }
            NotificationsMsg::Unavailable => {
                tracing::warn!("notifications applet: service unavailable");
            }
        }
    }
}

impl Notifications {
    fn refresh_indicator_widgets(&self, root: &gtk::Box) {
        root.set_tooltip_text(if self.tooltip.is_empty() {
            None
        } else {
            Some(&self.tooltip)
        });
        self.icon_widget.set_icon_name(Some(&self.icon_name));

        let presentation = badge_presentation(&self.badge_style, self.badge_count);
        let mut classes = vec!["notification-badge-anchor"];
        classes.extend_from_slice(presentation.css_classes);
        self.badge_widget.set_css_classes(&classes);
        self.badge_widget.set_visible(self.badge_visible);
        self.badge_value_label.set_visible(presentation.show_label);
        self.badge_value_label.set_label(&self.badge_label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::notifications::protocol::{NotificationEntry, NotificationsCommand};

    fn notif(id: u32, timestamp: u64, urgency: u8) -> NotificationEntry {
        NotificationEntry {
            id,
            app_name: format!("app-{id}"),
            app_icon: String::new(),
            desktop_entry: None,
            summary: format!("summary-{id}"),
            body: String::new(),
            urgency,
            actions: Vec::new(),
            image: None,
            timestamp,
            resident: false,
        }
    }

    #[test]
    fn filters_notifications_before_panel_start() {
        let notifications = vec![notif(1, 99, 1), notif(2, 100, 1), notif(3, 101, 2)];

        let fresh = filter_fresh_notifications(&notifications, 100);

        assert_eq!(fresh.len(), 2);
        assert_eq!(fresh[0].id, 2);
        assert_eq!(fresh[1].id, 3);
    }

    #[test]
    fn computes_badge_count_from_fresh_notifications_only() {
        let notifications = vec![notif(1, 100, 0), notif(2, 101, 1), notif(3, 102, 2)];

        let (count, badge_count) = notification_counts(&notifications);

        assert_eq!(count, 3);
        assert_eq!(badge_count, 3);
    }

    #[test]
    fn badge_count_includes_low_urgency_entries() {
        let notifications = vec![notif(1, 100, 0), notif(2, 101, 1)];

        assert_eq!(notification_counts(&notifications).1, 2);
    }

    #[test]
    fn badge_label_clamps_at_nine_plus() {
        assert_eq!(badge_presentation("count", 10).label, "9+");
        assert_eq!(badge_presentation("count", 1).label, "1");
        assert_eq!(badge_presentation("dot", 4).label, "");
    }

    #[test]
    fn badge_presentation_uses_label_only_for_count_mode() {
        assert!(badge_presentation("count", 3).show_label);
        assert!(!badge_presentation("dot", 3).show_label);
    }

    #[test]
    fn notification_action_command_maps_to_service_command() {
        let command = NotificationActionCommand::Dismiss { id: 42 };

        assert_eq!(
            command.into_service_command(),
            NotificationsCommand::Dismiss { id: 42 }
        );
    }
}
