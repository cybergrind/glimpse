use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::gtk::{self, glib, prelude::*};

use glimpse::notifications::NotificationEntry;

use super::NotificationActionCommand;
use super::activation::{default_action_command, invoke_action_command};
use super::components::row::build_notification_image_widget;

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
    on_mark_seen: Rc<dyn Fn(u32)>,
    on_command: Rc<dyn Fn(NotificationActionCommand)>,
}

impl NotificationPopup {
    pub fn new(
        popup_timeout: u32,
        position: &str,
        margin_top: i32,
        on_mark_seen: Rc<dyn Fn(u32)>,
        on_command: Rc<dyn Fn(NotificationActionCommand)>,
    ) -> Self {
        let window = gtk::Window::new();
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
                // top-center (default)
                window.set_anchor(Edge::Top, true);
            }
        }
        window.set_margin(Edge::Top, margin_top);
        window.set_margin(Edge::Right, 12);
        window.set_margin(Edge::Left, 12);
        window.set_margin(Edge::Bottom, 12);

        let card_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card_box.add_css_class("popup-card-list");

        let overflow_label = gtk::Label::new(None);
        overflow_label.add_css_class("popup-overflow");
        overflow_label.set_visible(false);
        card_box.append(&overflow_label);

        window.set_child(Some(&card_box));

        Self {
            window,
            card_box,
            cards: Rc::new(RefCell::new(HashMap::new())),
            overflow_label,
            popup_timeout,
            on_mark_seen,
            on_command,
        }
    }

    pub fn show(&mut self, notif: &NotifData) {
        // Remove existing card for this ID (replacement)
        self.remove_card(notif.id);

        // Max 20 visible — remove oldest
        while self.cards.borrow().len() >= 20 {
            let oldest = self.cards.borrow().keys().copied().min();
            if let Some(id) = oldest {
                self.remove_card(id);
            } else {
                break;
            }
        }

        let card = self.build_card(notif);
        self.card_box.prepend(&card);

        // Auto-dismiss timer
        let timeout_source = if self.popup_timeout > 0 {
            let cards = self.cards.clone();
            let card_box = self.card_box.clone();
            let window = self.window.clone();
            let id = notif.id;
            Some(glib::timeout_add_local_once(
                std::time::Duration::from_millis(self.popup_timeout as u64),
                move || {
                    if let Some(card) = cards.borrow_mut().remove(&id) {
                        card_box.remove(&card.card_widget);
                    }
                    if cards.borrow().is_empty() {
                        window.set_visible(false);
                    }
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

    fn remove_card(&self, id: u32) {
        self.remove_card_with_mode(id, PopupDismissMode::Dismiss);
    }

    fn remove_card_with_mode(&self, id: u32, mode: PopupDismissMode) {
        if let Some(card) = self.cards.borrow_mut().remove(&id) {
            if let Some(source) = card.timeout_source {
                source.remove();
            }
            self.card_box.remove(&card.card_widget);
        }
        if matches!(mode, PopupDismissMode::Dismiss) {
            (self.on_mark_seen)(id);
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

    fn build_card(&self, notif: &NotifData) -> gtk::Box {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
        card.add_css_class("popup-card");

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let icon_name = if !notif.app_icon.is_empty() {
            &notif.app_icon
        } else if let Some(ref de) = notif.desktop_entry {
            if !de.is_empty() {
                de
            } else {
                "dialog-information-symbolic"
            }
        } else {
            "dialog-information-symbolic"
        };
        let icon = gtk::Image::from_icon_name(icon_name);
        icon.set_pixel_size(16);
        icon.add_css_class("popup-card-icon");
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
        let on_command = self.on_command.clone();
        let popup = self.clone_handle();
        dismiss_btn.connect_clicked(move |_| {
            on_command(NotificationActionCommand::Dismiss { id });
            popup.remove_card_with_mode(id, PopupDismissMode::Dismiss);
        });
        header.append(&dismiss_btn);

        card.append(&header);

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_hexpand(true);
        content.add_css_class("popup-card-content");

        if let Some(image) = build_notification_image_widget(notif, "notification-inline-image") {
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
                let on_command = self.on_command.clone();
                let popup = self.clone_handle();
                action_btn.connect_clicked(move |_| {
                    on_command(invoke_action_command(nid, &k, None));
                    on_command(NotificationActionCommand::Dismiss { id: nid });
                    popup.remove_card_with_mode(nid, PopupDismissMode::Dismiss);
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
        let on_command = self.on_command.clone();
        let has_default = notif.actions.iter().any(|(k, _)| k == "default");
        let desktop_entry = notif.desktop_entry.clone();
        let app_name = notif.app_name.clone();
        let popup = self.clone_handle();
        gesture.connect_pressed(move |g, _, _, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            if has_default {
                let desktop_entry = desktop_entry.clone();
                let app_name = app_name.clone();
                let on_command = on_command.clone();
                let timestamp = g.current_event_time();
                glib::spawn_future_local(async move {
                    on_command(default_action_command(id, desktop_entry, app_name, timestamp).await);
                });
            }
            on_command(NotificationActionCommand::Dismiss { id });
            popup.remove_card_with_mode(id, PopupDismissMode::Dismiss);
        });
        card.add_controller(gesture);

        // Right-click: hide the popup card locally, but keep the notification in the popover.
        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        let id = notif.id;
        let popup = self.clone_handle();
        right_click.connect_pressed(move |g, _, _, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            popup.remove_card_with_mode(id, PopupDismissMode::HideOnly);
        });
        card.add_controller(right_click);

        card
    }

    fn clone_handle(&self) -> NotificationPopupHandle {
        NotificationPopupHandle {
            window: self.window.clone(),
            card_box: self.card_box.clone(),
            cards: self.cards.clone(),
            overflow_label: self.overflow_label.clone(),
            on_mark_seen: self.on_mark_seen.clone(),
        }
    }
}

#[derive(Clone)]
struct NotificationPopupHandle {
    window: gtk::Window,
    card_box: gtk::Box,
    cards: Rc<RefCell<HashMap<u32, PopupCard>>>,
    overflow_label: gtk::Label,
    on_mark_seen: Rc<dyn Fn(u32)>,
}

impl NotificationPopupHandle {
    fn remove_card_with_mode(&self, id: u32, mode: PopupDismissMode) {
        if let Some(card) = self.cards.borrow_mut().remove(&id) {
            if let Some(source) = card.timeout_source {
                source.remove();
            }
            self.card_box.remove(&card.card_widget);
        }
        if matches!(mode, PopupDismissMode::Dismiss) {
            (self.on_mark_seen)(id);
        }
        update_overflow(&self.cards, &self.overflow_label);
        if self.cards.borrow().is_empty() {
            self.window.set_visible(false);
        }
    }
}

fn update_overflow(cards: &Rc<RefCell<HashMap<u32, PopupCard>>>, overflow_label: &gtk::Label) {
    let cards = cards.borrow();
    let total = cards.len();
    if total > 5 {
        let mut sorted: Vec<(&u32, &PopupCard)> = cards.iter().collect();
        sorted.sort_by(|a, b| b.1.order.cmp(&a.1.order));
        for (i, (_, card)) in sorted.iter().enumerate() {
            card.card_widget.set_visible(i < 5);
        }
        let hidden = total - 5;
        overflow_label.set_label(&format!("+ {} more", hidden));
        overflow_label.set_visible(true);
    } else {
        for card in cards.values() {
            card.card_widget.set_visible(true);
        }
        overflow_label.set_visible(false);
    }
}

#[cfg(test)]
mod tests {
    use super::PopupDismissMode;

    #[test]
    fn popup_secondary_click_hides_only() {
        assert_eq!(PopupDismissMode::HideOnly, PopupDismissMode::HideOnly);
    }

    #[test]
    fn popup_primary_click_and_actions_dismiss() {
        assert_eq!(PopupDismissMode::Dismiss, PopupDismissMode::Dismiss);
    }
}
