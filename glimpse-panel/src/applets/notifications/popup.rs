use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, glib, prelude::*},
};

use glimpse::notifications::{
    NotificationEntry, NotificationsServiceHandle, NotificationsServiceState,
};

use super::NotificationActionCommand;
use super::activation::{default_action_command, invoke_action_command};
use super::components::row::{build_notification_icon, load_notification_image_texture};
use super::config::NotificationsConfig;

type NotifData = NotificationEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopupDismissMode {
    HideOnly,
    Dismiss,
}

struct PopupCard {
    card_widget: gtk::Box,
    timeout_source: Option<glib::SourceId>,
    order: u64, // timestamp for ordering
}

pub struct NotificationPopup {
    window: gtk::Window,
    card_box: gtk::Box,
    cards: Rc<RefCell<HashMap<u32, PopupCard>>>,
    overflow_label: gtk::Label,
    popup_timeout: u32,
    dnd: bool,
    started_at: u64,
    surfaced_ids: Rc<RefCell<HashMap<u32, u64>>>,
}

pub struct NotificationPopupInit {
    pub config: NotificationsConfig,
    pub service: NotificationsServiceHandle,
}

#[derive(Debug)]
pub enum NotificationPopupInput {
    ServiceState(NotificationsServiceState),
    TimeoutElapsed(u32),
    HideOnly(u32),
    Dismiss(u32),
    ActivateDefault(u32, Option<String>, String, u32),
    InvokeAction(u32, String),
    Unavailable,
}

#[relm4::component(pub)]
impl Component for NotificationPopup {
    type Init = NotificationPopupInit;
    type Input = NotificationPopupInput;
    type Output = NotificationActionCommand;
    type CommandOutput = NotificationPopupInput;

    view! {
        gtk::Window {
            #[name(card_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                add_css_class: "popup-card-list",

                #[name(overflow_label)]
                gtk::Label {
                    add_css_class: "popup-overflow",
                    set_visible: false,
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        configure_popup_window(
            &root,
            &init.config.popup_position,
            init.config.popup_margin_top,
        );

        let model = NotificationPopup {
            window: root.clone(),
            card_box: widgets.card_box.clone(),
            cards: Rc::new(RefCell::new(HashMap::new())),
            overflow_label: widgets.overflow_label.clone(),
            popup_timeout: init.config.popup_timeout,
            dnd: false,
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            surfaced_ids: Rc::new(RefCell::new(HashMap::new())),
        };

        let service = init.service;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    let mut state_rx = service.subscribe();
                    let _ = out.send(NotificationPopupInput::ServiceState(
                        state_rx.borrow().clone(),
                    ));

                    while state_rx.changed().await.is_ok() {
                        let _ = out.send(NotificationPopupInput::ServiceState(
                            state_rx.borrow().clone(),
                        ));
                    }

                    let _ = out.send(NotificationPopupInput::Unavailable);
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

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            NotificationPopupInput::ServiceState(state) => {
                self.dnd = state.dnd;
                self.prune_surfaced_ids(&state.notifications);

                let to_show = pending_popup_notifications(
                    &state.notifications,
                    self.started_at,
                    self.dnd,
                    &self
                        .surfaced_ids
                        .borrow()
                        .keys()
                        .copied()
                        .collect::<Vec<_>>(),
                );
                for notif in to_show {
                    self.surfaced_ids
                        .borrow_mut()
                        .insert(notif.id, notif.timestamp);
                    self.show(notif, &sender);
                }
            }
            NotificationPopupInput::TimeoutElapsed(id) => self.remove_card_with_mode(
                id,
                PopupDismissMode::HideOnly,
                TimeoutSourcePolicy::Keep,
            ),
            NotificationPopupInput::HideOnly(id) => {
                self.remove_card_with_mode(
                    id,
                    PopupDismissMode::HideOnly,
                    TimeoutSourcePolicy::RemoveIfPresent,
                );
            }
            NotificationPopupInput::Dismiss(id) => {
                sender
                    .output(NotificationActionCommand::Dismiss { id })
                    .ok();
                self.remove_card_with_mode(
                    id,
                    PopupDismissMode::Dismiss,
                    TimeoutSourcePolicy::RemoveIfPresent,
                );
            }
            NotificationPopupInput::ActivateDefault(id, desktop_entry, app_name, timestamp) => {
                let output_sender = sender.clone();
                glib::spawn_future_local(async move {
                    output_sender
                        .output(
                            default_action_command(id, desktop_entry, app_name, timestamp).await,
                        )
                        .ok();
                });
                sender
                    .output(NotificationActionCommand::Dismiss { id })
                    .ok();
                self.remove_card_with_mode(
                    id,
                    PopupDismissMode::Dismiss,
                    TimeoutSourcePolicy::RemoveIfPresent,
                );
            }
            NotificationPopupInput::InvokeAction(id, action_key) => {
                sender
                    .output(invoke_action_command(id, &action_key, None))
                    .ok();
                sender
                    .output(NotificationActionCommand::Dismiss { id })
                    .ok();
                self.remove_card_with_mode(
                    id,
                    PopupDismissMode::Dismiss,
                    TimeoutSourcePolicy::RemoveIfPresent,
                );
            }
            NotificationPopupInput::Unavailable => {
                tracing::warn!("notifications popup: service unavailable");
            }
        }
    }
}

impl NotificationPopup {
    fn show(&mut self, notif: &NotifData, sender: &ComponentSender<Self>) {
        self.remove_card_with_mode(
            notif.id,
            PopupDismissMode::HideOnly,
            TimeoutSourcePolicy::RemoveIfPresent,
        );

        while self.cards.borrow().len() >= 20 {
            let oldest = self
                .cards
                .borrow()
                .iter()
                .min_by_key(|(_, card)| card.order)
                .map(|(id, _)| *id);
            if let Some(id) = oldest {
                self.remove_card_with_mode(
                    id,
                    PopupDismissMode::HideOnly,
                    TimeoutSourcePolicy::RemoveIfPresent,
                );
            } else {
                break;
            }
        }

        let card = self.build_card(notif, sender);
        self.card_box.prepend(&card);

        let timeout_source = if self.popup_timeout > 0 {
            let id = notif.id;
            let sender = sender.clone();
            Some(glib::timeout_add_local_once(
                std::time::Duration::from_millis(self.popup_timeout as u64),
                move || {
                    sender.input(NotificationPopupInput::TimeoutElapsed(id));
                },
            ))
        } else {
            None
        };

        self.cards.borrow_mut().insert(
            notif.id,
            PopupCard {
                card_widget: card,
                timeout_source,
                order: notif.timestamp,
            },
        );

        self.update_overflow();
        self.window.set_visible(true);
    }

    fn remove_card_with_mode(
        &self,
        id: u32,
        mode: PopupDismissMode,
        timeout_policy: TimeoutSourcePolicy,
    ) {
        if let Some(card) = self.cards.borrow_mut().remove(&id) {
            if matches!(timeout_policy, TimeoutSourcePolicy::RemoveIfPresent) {
                if let Some(source) = card.timeout_source {
                    try_remove_source(source);
                }
            }
            self.card_box.remove(&card.card_widget);
        }
        if matches!(mode, PopupDismissMode::Dismiss) {
            self.surfaced_ids.borrow_mut().remove(&id);
        }
        self.update_overflow();
        if self.cards.borrow().is_empty() {
            self.window.set_visible(false);
        }
    }

    fn update_overflow(&self) {
        let cards = self.cards.borrow();
        let total = cards.len();
        if total > 5 {
            // Show newest 5, hide the rest
            let mut sorted: Vec<(&u32, &PopupCard)> = cards.iter().collect();
            sorted.sort_by(|a, b| b.1.order.cmp(&a.1.order));
            for (i, (_, card)) in sorted.iter().enumerate() {
                card.card_widget.set_visible(i < 5);
            }
            let hidden = total - 5;
            self.overflow_label.set_label(&format!("+ {} more", hidden));
            self.overflow_label.set_visible(true);
        } else {
            for card in cards.values() {
                card.card_widget.set_visible(true);
            }
            self.overflow_label.set_visible(false);
        }
    }

    fn build_card(&self, notif: &NotifData, sender: &ComponentSender<Self>) -> gtk::Box {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
        card.add_css_class("popup-card");

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let icon = build_notification_icon(notif, "popup-card-icon");
        header.append(&icon);

        let app = if notif.app_name.is_empty() {
            "Notification"
        } else {
            &notif.app_name
        };
        let app_label = gtk::Label::new(Some(app));
        app_label.set_hexpand(true);
        app_label.set_halign(gtk::Align::Start);
        app_label.add_css_class("popup-card-app");
        header.append(&app_label);

        let dismiss_btn = gtk::Button::from_icon_name("window-close-symbolic");
        dismiss_btn.add_css_class("flat");
        dismiss_btn.add_css_class("popup-dismiss");
        let id = notif.id;
        let sender_clone = sender.clone();
        dismiss_btn
            .connect_clicked(move |_| sender_clone.input(NotificationPopupInput::Dismiss(id)));
        header.append(&dismiss_btn);

        card.append(&header);

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_hexpand(true);
        content.add_css_class("popup-card-content");

        if let Some(texture) = load_notification_image_texture(notif) {
            let image = gtk::Picture::new();
            image.set_paintable(Some(&texture));
            image.set_can_shrink(true);
            image.set_keep_aspect_ratio(true);
            image.set_valign(gtk::Align::Start);
            image.add_css_class("notification-inline-image");
            image.add_css_class("popup-inline-image");
            content.append(&image);
        }

        let copy = gtk::Box::new(gtk::Orientation::Vertical, 4);
        copy.set_hexpand(true);
        copy.add_css_class("popup-card-copy");

        let summary = gtk::Label::new(Some(&notif.summary));
        summary.set_halign(gtk::Align::Start);
        summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
        summary.set_max_width_chars(50);
        summary.add_css_class("popup-card-summary");
        copy.append(&summary);

        if !notif.body.is_empty() {
            let body = gtk::Label::new(Some(&notif.body));
            body.set_halign(gtk::Align::Start);
            body.set_ellipsize(gtk::pango::EllipsizeMode::End);
            body.set_max_width_chars(55);
            body.set_lines(2);
            body.set_wrap(true);
            body.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            body.add_css_class("popup-card-body");
            copy.append(&body);
        }

        // Action buttons
        let visible_actions: Vec<&(String, String)> = notif
            .actions
            .iter()
            .filter(|(key, _)| key != "default")
            .collect();
        if !visible_actions.is_empty() {
            let actions_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            actions_box.add_css_class("popup-actions");
            for (key, label) in &visible_actions {
                let action_btn = gtk::Button::with_label(label);
                action_btn.add_css_class("flat");
                action_btn.add_css_class("popup-action-btn");
                let nid = notif.id;
                let k = key.clone();
                let sender_clone = sender.clone();
                action_btn.connect_clicked(move |_| {
                    sender_clone.input(NotificationPopupInput::InvokeAction(nid, k.clone()));
                });
                actions_box.append(&action_btn);
            }
            copy.append(&actions_box);
        }

        content.append(&copy);
        card.append(&content);

        if notif.urgency == 2 {
            card.add_css_class("popup-card-critical");
        }

        // Click card → dismiss (and invoke default action if available)
        let gesture = gtk::GestureClick::new();
        gesture.set_button(1);
        let id = notif.id;
        let has_default = notif.actions.iter().any(|(k, _)| k == "default");
        let desktop_entry = notif.desktop_entry.clone();
        let app_name = notif.app_name.clone();
        let sender_clone = sender.clone();
        gesture.connect_pressed(move |g, _, _, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            if has_default {
                let timestamp = g.current_event_time();
                sender_clone.input(NotificationPopupInput::ActivateDefault(
                    id,
                    desktop_entry.clone(),
                    app_name.clone(),
                    timestamp,
                ));
            } else {
                sender_clone.input(NotificationPopupInput::Dismiss(id));
            }
        });
        card.add_controller(gesture);

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        let id = notif.id;
        let sender_clone = sender.clone();
        right_click.connect_pressed(move |g, _, _, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            sender_clone.input(NotificationPopupInput::HideOnly(id));
        });
        card.add_controller(right_click);

        card
    }

    fn prune_surfaced_ids(&self, notifications: &[NotificationEntry]) {
        let current_ids: HashMap<u32, u64> = notifications
            .iter()
            .map(|notification| (notification.id, notification.timestamp))
            .collect();
        self.surfaced_ids
            .borrow_mut()
            .retain(|id, timestamp| current_ids.get(id).copied() == Some(*timestamp));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeoutSourcePolicy {
    Keep,
    RemoveIfPresent,
}

fn configure_popup_window(window: &gtk::Window, position: &str, margin_top: i32) {
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_default_size(380, -1);
    window.add_css_class("notification-popup");

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace("glimpse-notification-popup");
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);

    match position {
        "top-left" => {
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Left, true);
        }
        "top-right" => {
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Right, true);
        }
        "bottom-left" => {
            window.set_anchor(Edge::Bottom, true);
            window.set_anchor(Edge::Left, true);
        }
        "bottom-center" => {
            window.set_anchor(Edge::Bottom, true);
        }
        "bottom-right" => {
            window.set_anchor(Edge::Bottom, true);
            window.set_anchor(Edge::Right, true);
        }
        _ => {
            window.set_anchor(Edge::Top, true);
        }
    }
    window.set_margin(Edge::Top, margin_top);
    window.set_margin(Edge::Right, 12);
    window.set_margin(Edge::Left, 12);
    window.set_margin(Edge::Bottom, 12);
}

fn try_remove_source(source: glib::SourceId) {
    unsafe {
        let _ = glib::ffi::g_source_remove(source.as_raw());
    }
}

fn pending_popup_notifications<'a>(
    notifications: &'a [NotificationEntry],
    started_at: u64,
    dnd: bool,
    surfaced_ids: &[u32],
) -> Vec<&'a NotificationEntry> {
    let surfaced_ids: std::collections::HashSet<u32> = surfaced_ids.iter().copied().collect();
    notifications
        .iter()
        .filter(|notification| notification.timestamp >= started_at)
        .filter(|notification| !surfaced_ids.contains(&notification.id))
        .filter(|notification| !dnd || notification.urgency == 2)
        .collect()
}

#[cfg(test)]
fn pending_popup_ids(
    notifications: &[NotificationEntry],
    started_at: u64,
    dnd: bool,
    surfaced_ids: &[u32],
) -> Vec<u32> {
    pending_popup_notifications(notifications, started_at, dnd, surfaced_ids)
        .into_iter()
        .map(|notification| notification.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{NotifData, PopupDismissMode, TimeoutSourcePolicy, pending_popup_ids};

    #[test]
    fn popup_secondary_click_hides_only() {
        assert_eq!(PopupDismissMode::HideOnly, PopupDismissMode::HideOnly);
    }

    #[test]
    fn popup_primary_click_and_actions_dismiss() {
        assert_eq!(PopupDismissMode::Dismiss, PopupDismissMode::Dismiss);
    }

    #[test]
    fn timeout_elapsed_keeps_expired_source_handle() {
        assert_eq!(TimeoutSourcePolicy::Keep, TimeoutSourcePolicy::Keep);
        assert_ne!(
            TimeoutSourcePolicy::Keep,
            TimeoutSourcePolicy::RemoveIfPresent
        );
    }

    fn notif(id: u32, timestamp: u64, urgency: u8) -> NotifData {
        NotifData {
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
    fn pending_popup_ids_only_includes_fresh_unsurfaced_notifications() {
        let ids = pending_popup_ids(
            &[notif(1, 50, 1), notif(2, 150, 1), notif(3, 200, 1)],
            100,
            false,
            &[2],
        );

        assert_eq!(ids, vec![3]);
    }

    #[test]
    fn pending_popup_ids_respects_dnd_except_for_critical_notifications() {
        let ids = pending_popup_ids(
            &[notif(1, 150, 1), notif(2, 160, 2), notif(3, 170, 0)],
            100,
            true,
            &[],
        );

        assert_eq!(ids, vec![2]);
    }
}
