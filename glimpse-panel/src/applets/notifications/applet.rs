use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use serde::Deserialize;

use super::config::NotificationsConfig;
use super::popover::{NotificationsPopover, NotificationsPopoverInit, NotificationsPopoverInput};

pub struct Notifications {
    icon_name: String,
    badge_count: u32,
    badge_visible: bool,
    tooltip: String,
    popover: Controller<NotificationsPopover>,
}

pub struct NotificationsInit {
    pub config: NotificationsConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum NotificationsMsg {
    StatusUpdate { dnd: bool, count: u32 },
    ListUpdate(serde_json::Value),
    TogglePopover,
    Unavailable,
}

#[derive(Debug, Clone, Deserialize)]
struct NotifStatus {
    dnd: bool,
    count: u32,
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

            gtk::Overlay {
                gtk::Image {
                    #[watch]
                    set_icon_name: Some(&model.icon_name),
                    set_pixel_size: 16,
                },

                #[wrap(Some)]
                set_child = &gtk::Label {
                    #[watch]
                    set_label: &model.badge_count.to_string(),
                    #[watch]
                    set_visible: model.badge_visible,
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Start,
                    add_css_class: "notification-badge",
                },
            },
        }
    }

    fn init(
        init: Self::Init, root: Self::Root, sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = NotificationsPopover::builder()
            .launch(NotificationsPopoverInit {
                parent: root.clone(),
                client: init.client.clone(),
            })
            .detach();

        let model = Notifications {
            icon_name: "notifications-symbolic".into(),
            badge_count: 0,
            badge_visible: false,
            tooltip: "Notifications".into(),
            popover,
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
                                let _ = out.send(NotificationsMsg::StatusUpdate { dnd, count });
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
            NotificationsMsg::StatusUpdate { dnd, count } => {
                self.icon_name = if dnd {
                    "notifications-disabled-symbolic"
                } else {
                    "notifications-symbolic"
                }.into();

                self.badge_count = count;
                self.badge_visible = count > 0;

                self.tooltip = if dnd {
                    "Do Not Disturb".into()
                } else if count > 0 {
                    format!("{count} notification{}", if count > 1 { "s" } else { "" })
                } else {
                    "Notifications".into()
                };

                self.popover.emit(NotificationsPopoverInput::UpdateStatus { dnd, count });
            }
            NotificationsMsg::ListUpdate(data) => {
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
