use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use super::config::NotificationsConfig;
use super::popover::{NotifData, NotificationsPopover, NotificationsPopoverInit, NotificationsPopoverInput};
use super::popup::NotificationPopup;

pub struct Notifications {
    icon_name: String,
    badge_count: u32,
    badge_visible: bool,
    tooltip: String,
    dnd: bool,
    initialized: bool,
    popover: Controller<NotificationsPopover>,
    popup: Rc<RefCell<NotificationPopup>>,
    seen_ids: std::collections::HashSet<u32>,
}

pub struct NotificationsInit {
    pub config: NotificationsConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum NotificationsMsg {
    StatusUpdate { dnd: bool, count: u32, badge_count: u32 },
    ListUpdate(serde_json::Value),
    TogglePopover,
    Unavailable,
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
                set_label: &model.badge_count.to_string(),
                #[watch]
                set_visible: model.badge_visible,
                set_valign: gtk::Align::Start,
                add_css_class: "notification-badge",
            },
        }
    }

    fn init(
        init: Self::Init, root: Self::Root, sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        tracing::info!("notifications applet: initializing");
        let popover = NotificationsPopover::builder()
            .launch(NotificationsPopoverInit {
                parent: root.clone(),
                client: init.client.clone(),
            })
            .detach();

        let popup = Rc::new(RefCell::new(NotificationPopup::new(
            init.client.clone(),
            init.config.popup_timeout,
            &init.config.popup_position,
        )));

        let model = Notifications {
            icon_name: "preferences-system-notifications-symbolic".into(),
            badge_count: 0,
            badge_visible: false,
            tooltip: "Notifications".into(),
            dnd: false,
            initialized: false,
            popover,
            popup,
            seen_ids: std::collections::HashSet::new(),
        };

        let client = init.client;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("notifications applet: subscribing");
                    let mut status_sub = match client.subscribe("notifications.status").await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("notifications: subscribe failed: {e}");
                            let _ = out.send(NotificationsMsg::Unavailable);
                            return;
                        }
                    };
                    let mut list_sub = client.subscribe("notifications.list").await.ok();

                    loop {
                        tokio::select! {
                            Some(ev) = status_sub.next() => {
                                let dnd = ev.data["dnd"].as_bool().unwrap_or(false);
                                let count = ev.data["count"].as_u64().unwrap_or(0) as u32;
                                let badge_count = ev.data["badge_count"].as_u64().unwrap_or(0) as u32;
                                let _ = out.send(NotificationsMsg::StatusUpdate { dnd, count, badge_count });
                            }
                            Some(ev) = async {
                                match &mut list_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                let _ = out.send(NotificationsMsg::ListUpdate(ev.data));
                            }
                            else => break,
                        }
                    }
                    let _ = out.send(NotificationsMsg::Unavailable);
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(&mut self, msg: Self::CommandOutput, sender: ComponentSender<Self>, root: &Self::Root) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            NotificationsMsg::StatusUpdate { dnd, count, badge_count } => {
                tracing::info!(count, badge_count, dnd, "notifications applet: status update");
                self.dnd = dnd;
                self.icon_name = if dnd {
                    "notifications-disabled-symbolic"
                } else {
                    "preferences-system-notifications-symbolic"
                }.into();

                self.badge_count = badge_count;
                self.badge_visible = badge_count > 0;

                self.tooltip = if dnd {
                    "Do Not Disturb".into()
                } else if count > 0 {
                    format!("{count} notification{}", if count > 1 { "s" } else { "" })
                } else {
                    "Notifications".into()
                };

                self.popover.emit(NotificationsPopoverInput::UpdateStatus { dnd, count, badge_count });
            }
            NotificationsMsg::ListUpdate(data) => {
                if let Some(arr) = data.as_array() {
                    if !self.initialized {
                        // First update — mark all existing as seen, no popups
                        for notif_val in arr {
                            if let Some(id) = notif_val["id"].as_u64() {
                                self.seen_ids.insert(id as u32);
                            }
                        }
                        self.initialized = true;
                    } else {
                        // Subsequent updates — show popups for new notifications
                        for notif_val in arr {
                            if let Some(id) = notif_val["id"].as_u64() {
                                let id = id as u32;
                                if !self.seen_ids.contains(&id) {
                                    self.seen_ids.insert(id);
                                    let urgency = notif_val["urgency"].as_u64().unwrap_or(1) as u8;
                                    if !self.dnd || urgency == 2 {
                                        if let Ok(notif) = serde_json::from_value::<NotifData>(notif_val.clone()) {
                                            self.popup.borrow_mut().show(&notif);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Clean up seen_ids for dismissed notifications
                    let current_ids: std::collections::HashSet<u32> = arr.iter()
                        .filter_map(|n| n["id"].as_u64().map(|id| id as u32))
                        .collect();
                    self.seen_ids.retain(|id| current_ids.contains(id));
                }
                self.popover.emit(NotificationsPopoverInput::UpdateList(data));
            }
            NotificationsMsg::TogglePopover => {
                self.popover.emit(NotificationsPopoverInput::Toggle);
            }
            NotificationsMsg::Unavailable => {
                tracing::warn!("notifications applet: daemon unavailable");
            }
        }
    }
}
