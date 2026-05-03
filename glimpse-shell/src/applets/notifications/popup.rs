#![allow(unused_assignments)]

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, gdk, glib, prelude::*},
};
use serde::Deserialize;

use crate::services::notifications::model::NotificationEntry;

use super::{
    components::{
        NotificationActionButton, NotificationActionButtonInit, NotificationActionButtonStyle,
    },
    format, popover,
};

const MAX_VISIBLE_POPUPS: usize = 5;
const MAX_TRACKED_POPUPS: usize = 20;

pub struct Popup {
    window: gtk::Window,
    card_box: gtk::Box,
    overflow: gtk::Label,
    timeout_ms: u32,
    started_at: u64,
    surfaced: HashMap<u32, u64>,
    cards: Rc<RefCell<HashMap<u32, PopupCard>>>,
}

struct PopupCard {
    widget: gtk::Box,
    timeout: Option<glib::SourceId>,
    order: u64,
}

pub struct PopupInit {
    pub timeout_ms: u32,
    pub position: PopupPosition,
    pub margin_x: i32,
    pub margin_y: i32,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PopupPosition {
    #[serde(alias = "top-left")]
    TopLeft,
    #[serde(alias = "top-center")]
    TopCenter,
    #[serde(alias = "top-right")]
    TopRight,
    #[serde(alias = "bottom-left")]
    BottomLeft,
    #[serde(alias = "bottom-center")]
    BottomCenter,
    #[serde(alias = "bottom-right")]
    BottomRight,
}

impl Default for PopupPosition {
    fn default() -> Self {
        Self::TopCenter
    }
}

#[derive(Debug)]
pub enum PopupInput {
    Update {
        notifications: Vec<NotificationEntry>,
        dnd: bool,
    },
    Reconfigure {
        timeout_ms: u32,
        position: PopupPosition,
        margin_x: i32,
        margin_y: i32,
    },
    TimeoutElapsed(u32),
    Cancel(u32),
    Dismiss(u32),
    FocusAndDismiss(u32),
    InvokeAction {
        id: u32,
        action_key: String,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for Popup {
    type Init = PopupInit;
    type Input = PopupInput;
    type Output = popover::PopoverOutput;

    view! {
        root = gtk::Window {
            #[name = "card_box"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                add_css_class: "popup-card-list",

                #[name = "overflow"]
                gtk::Label {
                    add_css_class: "popup-overflow",
                    set_visible: false,
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        configure_window(&widgets.root, init.position, init.margin_x, init.margin_y);
        let model = Popup {
            window: widgets.root.clone(),
            card_box: widgets.card_box.clone(),
            overflow: widgets.overflow.clone(),
            timeout_ms: init.timeout_ms,
            started_at: now_ms(),
            surfaced: HashMap::new(),
            cards: Rc::new(RefCell::new(HashMap::new())),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopupInput::Update { notifications, dnd } => {
                self.prune_surfaced(&notifications);
                if dnd {
                    self.mark_surfaced(&notifications);
                    self.hide_all();
                    return;
                }

                let started_at = self.started_at;
                let shown = self.surfaced.keys().copied().collect::<HashSet<_>>();
                let pending = notifications
                    .iter()
                    .filter(|item| item.timestamp >= started_at)
                    .filter(|item| !shown.contains(&item.id))
                    .cloned()
                    .collect::<Vec<_>>();
                for notification in pending {
                    self.surfaced
                        .insert(notification.id, notification.timestamp);
                    if notification.timestamp >= started_at {
                        self.show(&notification, &sender);
                    }
                }
            }
            PopupInput::Reconfigure {
                timeout_ms,
                position,
                margin_x,
                margin_y,
            } => {
                self.timeout_ms = timeout_ms;
                apply_position(&self.window, position, margin_x, margin_y);
            }
            PopupInput::TimeoutElapsed(id) => self.remove_card(id, false),
            PopupInput::Cancel(id) => self.remove_card(id, true),
            PopupInput::Dismiss(id) => {
                let _ = sender.output(popover::PopoverOutput::Dismiss(id));
                self.remove_card(id, true);
            }
            PopupInput::FocusAndDismiss(id) => {
                let _ = sender.output(popover::PopoverOutput::FocusAndDismiss(id));
                self.remove_card(id, true);
            }
            PopupInput::InvokeAction { id, action_key } => {
                let _ = sender.output(popover::PopoverOutput::InvokeAction { id, action_key });
                self.remove_card(id, true);
            }
        }
    }
}

impl Drop for Popup {
    fn drop(&mut self) {
        for (_, card) in self.cards.borrow_mut().drain() {
            if let Some(timeout) = card.timeout {
                timeout.remove();
            }
        }
    }
}

impl Popup {
    fn show(&mut self, notification: &NotificationEntry, sender: &ComponentSender<Self>) {
        self.remove_card(notification.id, true);
        while self.cards.borrow().len() >= MAX_TRACKED_POPUPS {
            let oldest = self
                .cards
                .borrow()
                .iter()
                .min_by_key(|(_, card)| card.order)
                .map(|(id, _)| *id);
            let Some(id) = oldest else {
                break;
            };
            self.remove_card(id, true);
        }

        let card = build_card(notification, sender);
        self.card_box.prepend(&card);
        let timeout = if self.timeout_ms > 0 {
            let id = notification.id;
            let sender = sender.clone();
            Some(glib::timeout_add_local_once(
                Duration::from_millis(self.timeout_ms as u64),
                move || sender.input(PopupInput::TimeoutElapsed(id)),
            ))
        } else {
            None
        };

        self.cards.borrow_mut().insert(
            notification.id,
            PopupCard {
                widget: card,
                timeout,
                order: notification.timestamp,
            },
        );
        self.update_overflow();
        self.window.set_visible(true);
    }

    fn remove_card(&mut self, id: u32, remove_timeout: bool) {
        if let Some(card) = self.cards.borrow_mut().remove(&id) {
            if remove_timeout {
                if let Some(timeout) = card.timeout {
                    timeout.remove();
                }
            }
            self.card_box.remove(&card.widget);
        }

        self.update_overflow();
        if self.cards.borrow().is_empty() {
            self.window.set_visible(false);
        }
    }

    fn hide_all(&mut self) {
        let ids = self.cards.borrow().keys().copied().collect::<Vec<_>>();
        for id in ids {
            self.remove_card(id, true);
        }
    }

    fn update_overflow(&self) {
        let cards = self.cards.borrow();
        let mut sorted = cards.values().collect::<Vec<_>>();
        sorted.sort_by(|a, b| b.order.cmp(&a.order));
        for (index, card) in sorted.iter().enumerate() {
            card.widget.set_visible(index < MAX_VISIBLE_POPUPS);
        }

        let hidden = cards.len().saturating_sub(MAX_VISIBLE_POPUPS);
        self.overflow.set_visible(hidden > 0);
        if hidden > 0 {
            self.overflow.set_label(&format!("+ {hidden} more"));
        }
    }

    fn prune_surfaced(&mut self, notifications: &[NotificationEntry]) {
        let active = notifications
            .iter()
            .map(|notification| (notification.id, notification.timestamp))
            .collect::<HashMap<_, _>>();
        self.surfaced
            .retain(|id, timestamp| active.get(id).copied() == Some(*timestamp));
    }

    fn mark_surfaced(&mut self, notifications: &[NotificationEntry]) {
        for notification in notifications
            .iter()
            .filter(|notification| notification.timestamp >= self.started_at)
        {
            self.surfaced
                .insert(notification.id, notification.timestamp);
        }
    }
}

struct PopupCardInit {
    icon_name: String,
    app_name: String,
    summary: String,
    body: String,
}

#[relm4::widget_template]
impl WidgetTemplate for PopupCardView {
    type Init = PopupCardInit;

    view! {
        gtk::Box {
            add_css_class: "popup-card",
            add_css_class: "card-surface",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 6,

            gtk::Box {
                add_css_class: "card-surface__header",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Image {
                    add_css_class: "popup-card-icon",
                    set_icon_name: Some(&init.icon_name),
                },

                gtk::Label {
                    add_css_class: "popup-card-app",
                    set_label: &init.app_name,
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                },

                #[name = "dismiss"]
                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "popup-dismiss",
                    set_icon_name: "window-close-symbolic",
                },
            },

            gtk::Box {
                add_css_class: "popup-card-content",
                add_css_class: "card-surface__body",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,

                #[name = "image"]
                gtk::Picture {
                    add_css_class: "notification-inline-image",
                    add_css_class: "popup-inline-image",
                    set_can_shrink: true,
                    set_content_fit: gtk::ContentFit::Contain,
                    set_valign: gtk::Align::Start,
                    set_visible: false,
                },

                gtk::Box {
                    add_css_class: "popup-card-copy",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 4,
                    set_hexpand: true,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,

                        gtk::Label {
                            add_css_class: "popup-card-summary",
                            set_label: &init.summary,
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_max_width_chars: 50,
                        },

                        gtk::Label {
                            add_css_class: "popup-card-body",
                            set_label: &init.body,
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_max_width_chars: 55,
                            set_lines: 2,
                            set_wrap: true,
                            set_wrap_mode: gtk::pango::WrapMode::WordChar,
                            set_visible: !init.body.is_empty(),
                        },
                    },

                    #[name = "actions_box"]
                    gtk::Box {
                        add_css_class: "popup-actions",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 4,
                        set_visible: false,
                    },
                },
            },
        }
    }
}

fn build_card(notification: &NotificationEntry, sender: &ComponentSender<Popup>) -> gtk::Box {
    let card = PopupCardView::init(PopupCardInit {
        icon_name: popup_icon_name(notification).into(),
        app_name: format::source_name(notification).into(),
        summary: notification.summary.clone(),
        body: notification.body.clone(),
    });
    if notification.urgency == 2 {
        card.as_ref().add_css_class("popup-card-critical");
    }

    let id = notification.id;
    let sender_clone = sender.clone();
    card.dismiss
        .connect_clicked(move |_| sender_clone.input(PopupInput::Dismiss(id)));

    let root_click = gtk::GestureClick::new();
    root_click.set_button(1);
    root_click.set_propagation_phase(gtk::PropagationPhase::Bubble);
    let id = notification.id;
    let sender_clone = sender.clone();
    let card_widget = card.as_ref().clone();
    let dismiss = card.dismiss.clone();
    let actions_box = card.actions_box.clone();
    root_click.connect_pressed(move |gesture, _, x, y| {
        if point_inside_widget(&card_widget, &dismiss, x, y)
            || point_inside_widget(&card_widget, &actions_box, x, y)
        {
            return;
        }
        gesture.set_state(gtk::EventSequenceState::Claimed);
        sender_clone.input(PopupInput::FocusAndDismiss(id));
    });
    card.as_ref().add_controller(root_click);

    if let Some(texture) = load_notification_image(notification) {
        card.image.set_paintable(Some(&texture));
        card.image.set_visible(true);
    } else {
        card.image.set_paintable(None::<&gdk::Texture>);
        card.image.set_visible(false);
    }

    let actions = format::visible_actions(notification).collect::<Vec<_>>();
    for (action_key, label) in actions {
        let button = NotificationActionButton::init(NotificationActionButtonInit {
            label: label.into(),
            style: NotificationActionButtonStyle::Popup,
        });
        let id = notification.id;
        let action_key = action_key.to_string();
        let sender_clone = sender.clone();
        button.as_ref().connect_clicked(move |_| {
            sender_clone.input(PopupInput::InvokeAction {
                id,
                action_key: action_key.clone(),
            });
        });
        card.actions_box.append(button.as_ref());
    }
    card.actions_box
        .set_visible(card.actions_box.first_child().is_some());

    let right_click = gtk::GestureClick::new();
    right_click.set_button(3);
    right_click.set_propagation_phase(gtk::PropagationPhase::Bubble);
    let id = notification.id;
    let sender_clone = sender.clone();
    let card_widget = card.as_ref().clone();
    let dismiss = card.dismiss.clone();
    let actions_box = card.actions_box.clone();
    right_click.connect_pressed(move |gesture, _, x, y| {
        if point_inside_widget(&card_widget, &dismiss, x, y)
            || point_inside_widget(&card_widget, &actions_box, x, y)
        {
            return;
        }
        gesture.set_state(gtk::EventSequenceState::Claimed);
        sender_clone.input(PopupInput::Cancel(id));
    });
    card.as_ref().add_controller(right_click);

    card.as_ref().clone()
}

fn popup_icon_name(notification: &NotificationEntry) -> &str {
    if notification.app_icon.is_empty() {
        "dialog-information-symbolic"
    } else {
        notification.app_icon.as_str()
    }
}

fn load_notification_image(notification: &NotificationEntry) -> Option<gdk::Texture> {
    let image = notification.image.as_deref()?.trim();
    if image.is_empty() {
        return None;
    }

    if let Some(path) = image.strip_prefix("file://") {
        return gdk::Texture::from_filename(path).ok();
    }

    if image.starts_with('/') {
        return gdk::Texture::from_filename(image).ok();
    }

    None
}

fn point_inside_widget(
    source: &impl IsA<gtk::Widget>,
    target: &impl IsA<gtk::Widget>,
    x: f64,
    y: f64,
) -> bool {
    source
        .translate_coordinates(target, x, y)
        .map(|(x, y)| target.contains(x, y))
        .unwrap_or(false)
}

fn configure_window(window: &gtk::Window, position: PopupPosition, margin_x: i32, margin_y: i32) {
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_default_size(380, -1);
    window.add_css_class("notification-popup");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace("glimpse-notification-popup");
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    apply_position(window, position, margin_x, margin_y);
}

fn apply_position(window: &gtk::Window, position: PopupPosition, margin_x: i32, margin_y: i32) {
    for edge in [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left] {
        window.set_anchor(edge, false);
        window.set_margin(edge, 0);
    }

    match position {
        PopupPosition::TopLeft => {
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Left, true);
            window.set_margin(Edge::Top, margin_y);
            window.set_margin(Edge::Left, margin_x);
        }
        PopupPosition::TopCenter => {
            window.set_anchor(Edge::Top, true);
            window.set_margin(Edge::Top, margin_y);
        }
        PopupPosition::TopRight => {
            window.set_anchor(Edge::Top, true);
            window.set_anchor(Edge::Right, true);
            window.set_margin(Edge::Top, margin_y);
            window.set_margin(Edge::Right, margin_x);
        }
        PopupPosition::BottomLeft => {
            window.set_anchor(Edge::Bottom, true);
            window.set_anchor(Edge::Left, true);
            window.set_margin(Edge::Bottom, margin_y);
            window.set_margin(Edge::Left, margin_x);
        }
        PopupPosition::BottomCenter => {
            window.set_anchor(Edge::Bottom, true);
            window.set_margin(Edge::Bottom, margin_y);
        }
        PopupPosition::BottomRight => {
            window.set_anchor(Edge::Bottom, true);
            window.set_anchor(Edge::Right, true);
            window.set_margin(Edge::Bottom, margin_y);
            window.set_margin(Edge::Right, margin_x);
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn popup_card_view_exposes_stable_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let card = PopupCardView::init(PopupCardInit {
            icon_name: "dialog-information-symbolic".into(),
            app_name: "App".into(),
            summary: "Summary".into(),
            body: String::new(),
        });

        assert!(card.as_ref().has_css_class("popup-card"));
        assert!(card.as_ref().has_css_class("card-surface"));
        assert!(card.dismiss.has_css_class("popup-dismiss"));
        assert!(card.image.has_css_class("notification-inline-image"));
        assert!(card.image.has_css_class("popup-inline-image"));
        assert!(card.actions_box.has_css_class("popup-actions"));
        assert!(!card.actions_box.is_visible());
    }
}
