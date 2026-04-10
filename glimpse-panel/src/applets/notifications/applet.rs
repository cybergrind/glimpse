use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::config::NotificationsConfig;
use super::NotificationActionCommand;
use super::popover::{
    NotificationsPopover, NotificationsPopoverInit, NotificationsPopoverInput,
};
use super::popup::NotificationPopup;
use glimpse::notifications::{NotificationEntry, NotificationsServiceHandle, NotificationsServiceState};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

pub struct Notifications {
    icon_name: String,
    badge_label: String,
    badge_visible: bool,
    badge_style: String, // "count", "dot", ""
    tooltip: String,
    service: NotificationsServiceHandle,
    dnd: bool,
    started_at: u64,
    popover: Controller<NotificationsPopover>,
    popup: Rc<RefCell<NotificationPopup>>,
    surfaced_ids: HashSet<u32>,
    unread: HashMap<u32, u8>,
}

pub struct NotificationsInit {
    pub config: NotificationsConfig,
    pub service: NotificationsServiceHandle,
}

#[derive(Debug)]
pub enum NotificationsMsg {
    ServiceState(NotificationsServiceState),
    MarkSeen(u32),
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
    let badge_count = notifications
        .iter()
        .filter(|notification| notification.urgency > 0)
        .count() as u32;
    (count, badge_count)
}

fn unread_badge_count(unread: &HashMap<u32, u8>) -> u32 {
    unread.values().filter(|&&urgency| urgency > 0).count() as u32
}

fn badge_label(style: &str, badge_count: u32) -> String {
    match style {
        "count" => {
            if badge_count > 9 {
                "9+".into()
            } else {
                badge_count.to_string()
            }
        }
        "dot" | _ => String::new(),
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

#[relm4::component(pub)]
impl Component for Notifications {
    type Init = NotificationsInit;
    type Input = NotificationsMsg;
    type Output = ();
    type CommandOutput = NotificationsMsg;

    view! {
        gtk::Box {
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "notifications",
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(NotificationsMsg::TogglePopover);
                }
            },

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 16,
            },

            gtk::Label {
                #[watch]
                set_label: &model.badge_label,
                #[watch]
                set_visible: model.badge_visible,
                set_valign: gtk::Align::Center,
                set_halign: gtk::Align::Center,
                #[watch]
                set_css_classes: if model.badge_style == "count" {
                    &["notification-badge"]
                } else {
                    &["notification-dot"]
                },
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

        let popup = Rc::new(RefCell::new(NotificationPopup::new(
            init.config.popup_timeout,
            &init.config.popup_position,
            init.config.popup_margin_top,
            Rc::new({
                let sender = sender.clone();
                move |id| sender.input(NotificationsMsg::MarkSeen(id))
            }),
            emit_command,
        )));

        let model = Notifications {
            icon_name: "preferences-system-notifications-symbolic".into(),
            badge_label: String::new(),
            badge_visible: false,
            badge_style: init.config.badge_style.clone(),
            tooltip: "Notifications".into(),
            service: init.service.clone(),
            dnd: false,
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            popover,
            popup,
            surfaced_ids: HashSet::new(),
            unread: HashMap::new(),
        };

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

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            NotificationsMsg::ServiceState(state) => {
                let dnd = state.dnd;
                tracing::info!(dnd, "notifications applet: status update");
                self.dnd = dnd;
                self.icon_name = if dnd {
                    "notifications-disabled-symbolic"
                } else {
                    "preferences-system-notifications-symbolic"
                }
                .into();

                let fresh = filter_fresh_notifications(&state.notifications, self.started_at);
                let (count, _) = notification_counts(&fresh);

                for notif in &fresh {
                    let id = notif.id;
                    if !self.surfaced_ids.contains(&id) {
                        self.surfaced_ids.insert(id);
                        self.unread.entry(id).or_insert(notif.urgency);
                        if !self.dnd || notif.urgency == 2 {
                            self.popup.borrow_mut().show(notif);
                        }
                    }
                }

                let current_ids: std::collections::HashSet<u32> =
                    fresh.iter().map(|notification| notification.id).collect();
                self.surfaced_ids.retain(|id| current_ids.contains(id));

                let badge_count = unread_badge_count(&self.unread);
                self.badge_visible = badge_count > 0 && !self.badge_style.is_empty();
                self.badge_label = badge_label(&self.badge_style, badge_count);
                self.tooltip = tooltip_text(self.dnd, count);

                self.popover.emit(NotificationsPopoverInput::UpdateStatus {
                    dnd: self.dnd,
                    count,
                    badge_count,
                });
                self.popover
                    .emit(NotificationsPopoverInput::UpdateList(fresh));
            }
            NotificationsMsg::MarkSeen(id) => {
                self.unread.remove(&id);
                let badge_count = unread_badge_count(&self.unread);
                self.badge_visible = badge_count > 0 && !self.badge_style.is_empty();
                self.badge_label = badge_label(&self.badge_style, badge_count);
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
        assert_eq!(badge_count, 2);
    }

    #[test]
    fn unread_badge_count_uses_unread_state_not_active_list() {
        let unread = HashMap::from([(1, 1), (2, 2)]);

        assert_eq!(unread_badge_count(&unread), 2);
    }

    #[test]
    fn unread_badge_count_ignores_low_urgency_entries() {
        let unread = HashMap::from([(1, 0), (2, 1)]);

        assert_eq!(unread_badge_count(&unread), 1);
    }

    #[test]
    fn badge_label_clamps_at_nine_plus() {
        assert_eq!(badge_label("count", 10), "9+");
        assert_eq!(badge_label("count", 1), "1");
        assert_eq!(badge_label("dot", 4), "");
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
