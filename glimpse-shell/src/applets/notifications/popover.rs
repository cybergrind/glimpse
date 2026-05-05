#![allow(unused_assignments)]

use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, gdk, glib, prelude::*},
};

use crate::{
    components::{
        animated_popover::AnimatedPopover, hero::HeroView, popover_scroll,
        popover_shell::PopoverShell,
    },
    services::notifications::model::NotificationEntry,
};
use glimpse_core::services::notifications::model::State as NotificationState;

use super::{
    components::{
        NotificationActionButton, NotificationActionButtonInit, NotificationActionButtonStyle,
        NotificationGroup, NotificationGroupAction, NotificationListItem, notification_items,
    },
    format,
};

pub struct Popover {
    animation: AnimatedPopover,
    refresh_timer: Option<glib::SourceId>,
    notifications: Vec<NotificationEntry>,
    dnd: bool,
    groups: HashMap<String, NotificationGroup>,
    rows: HashMap<u32, NotificationRow>,
    list: gtk::Box,
    scroller: gtk::ScrolledWindow,
    empty: gtk::Box,
    hero_icon: gtk::Image,
    hero_subtitle: gtk::Label,
    hero_toggle: gtk::Switch,
    updating_dnd: Rc<Cell<bool>>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    Update {
        notifications: Vec<NotificationEntry>,
        dnd: bool,
    },
    Dismiss(u32),
    DismissAll,
    SetDnd(bool),
    FocusAndDismiss(u32),
    DismissGroup(Vec<u32>),
    ToggleGroup(String),
    RefreshTimes,
    InvokeAction {
        id: u32,
        action_key: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Opened,
    Closed,
    Dismiss(u32),
    DismissAll,
    DismissMany(Vec<u32>),
    SetDnd(bool),
    FocusAndDismiss(u32),
    InvokeAction { id: u32, action_key: String },
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "notifications-popover",
            add_css_class: "popover-size-large",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                content {
                    #[name = "hero"]
                    #[template]
                    HeroView {},

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[name = "empty"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_vexpand: true,
                        set_hexpand: true,
                        add_css_class: "empty-state",

                        gtk::Label {
                            add_css_class: "empty-state__title",
                            set_label: "No notifications",
                        },

                        gtk::Label {
                            add_css_class: "empty-state__subtitle",
                            set_label: "You're caught up.",
                        },
                    },

                    #[name = "scroller"]
                    gtk::ScrolledWindow {
                        set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                        set_vexpand: true,
                        set_propagate_natural_height: true,

                        #[name = "list"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 4,
                            add_css_class: "notification-list",
                        }
                    },
                },

                #[template_child]
                footer {
                    gtk::Button {
                        add_css_class: "flat",
                        add_css_class: "footer-action",
                        set_label: "Clear All",
                        connect_clicked => PopoverInput::DismissAll,
                    }
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        popover_scroll::install_half_monitor_limit(&widgets.root, &widgets.scroller, &init.parent);

        let opened_sender = _sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(PopoverOutput::Opened);
        });

        let closed_sender = _sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(PopoverOutput::Closed);
        });

        widgets
            .hero
            .icon
            .set_icon_name(Some(notification_popover_icon_name(false)));
        widgets.hero.title.set_label("Notifications");
        widgets.hero.subtitle.set_label("No notifications");
        widgets.hero.trailing.set_visible(true);

        let updating_dnd = Rc::new(Cell::new(false));
        widgets.hero.toggle.connect_state_set({
            let sender = _sender.clone();
            let updating_dnd = updating_dnd.clone();
            move |_, active| {
                if !updating_dnd.get() {
                    sender.input(PopoverInput::SetDnd(!active));
                }
                glib::Propagation::Proceed
            }
        });

        let refresh_timer = glib::timeout_add_seconds_local(60, {
            let sender = _sender.input_sender().clone();
            let root = widgets.root.clone();
            move || {
                if root.is_visible() {
                    let _ = sender.send(PopoverInput::RefreshTimes);
                }
                glib::ControlFlow::Continue
            }
        });

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            refresh_timer: Some(refresh_timer),
            notifications: Vec::new(),
            dnd: false,
            groups: HashMap::new(),
            rows: HashMap::new(),
            list: widgets.list.clone(),
            scroller: widgets.scroller.clone(),
            empty: widgets.empty.clone(),
            hero_icon: widgets.hero.icon.clone(),
            hero_subtitle: widgets.hero.subtitle.clone(),
            hero_toggle: widgets.hero.toggle.clone(),
            updating_dnd,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.animation.toggle(),
            PopoverInput::Update { notifications, dnd } => {
                self.notifications = notifications;
                self.dnd = dnd;
                self.sync(&sender);
            }
            PopoverInput::Dismiss(id) => {
                let _ = sender.output(PopoverOutput::Dismiss(id));
            }
            PopoverInput::DismissAll => {
                let _ = sender.output(PopoverOutput::DismissAll);
            }
            PopoverInput::SetDnd(enabled) => {
                let _ = sender.output(PopoverOutput::SetDnd(enabled));
            }
            PopoverInput::FocusAndDismiss(id) => {
                let _ = sender.output(PopoverOutput::FocusAndDismiss(id));
            }
            PopoverInput::DismissGroup(ids) => {
                let _ = sender.output(PopoverOutput::DismissMany(ids));
            }
            PopoverInput::ToggleGroup(key) => {
                if let Some(group) = self.groups.get(&key) {
                    group.toggle();
                }
                self.sync(&sender);
            }
            PopoverInput::RefreshTimes => self.refresh_times(),
            PopoverInput::InvokeAction { id, action_key } => {
                let _ = sender.output(PopoverOutput::InvokeAction { id, action_key });
            }
        }
    }
}

impl Popover {
    fn sync(&mut self, sender: &ComponentSender<Self>) {
        let now = now_ms();
        let items = notification_items(&self.notifications);
        let mut seen_groups = HashSet::new();
        let mut seen_rows = HashSet::new();
        let mut previous: Option<gtk::Widget> = None;
        self.empty.set_visible(self.notifications.is_empty());
        self.scroller.set_visible(!self.notifications.is_empty());
        self.hero_icon
            .set_icon_name(Some(notification_popover_icon_name(self.dnd)));
        let subtitle = if self.dnd {
            "Do Not Disturb".into()
        } else {
            format::count_label(self.notifications.len())
        };
        self.hero_subtitle.set_label(&subtitle);
        self.updating_dnd.set(true);
        self.hero_toggle.set_active(!self.dnd);
        self.updating_dnd.set(false);

        for item in &items {
            match item {
                NotificationListItem::Notification(notification) => {
                    seen_rows.insert(notification.id);
                    let row = self
                        .rows
                        .entry(notification.id)
                        .or_insert_with(|| NotificationRow::new(sender));
                    row.update(notification, now, sender);
                    row.set_header_visible(true);
                    row.set_card_actions_enabled(true);
                    place_row(row, &self.list, previous.as_ref());
                    previous = Some(row.root.as_ref().clone().upcast());
                }
                NotificationListItem::Group(group_model) => {
                    seen_groups.insert(group_model.key.clone());
                    let group = self
                        .groups
                        .entry(group_model.key.clone())
                        .or_insert_with(|| {
                            let sender = sender.clone();
                            NotificationGroup::new(&group_model.key, move |action| match action {
                                NotificationGroupAction::Dismiss(ids) => {
                                    sender.input(PopoverInput::DismissGroup(ids));
                                }
                                NotificationGroupAction::Toggle(key) => {
                                    sender.input(PopoverInput::ToggleGroup(key));
                                }
                            })
                        });
                    if group.root_widget().parent().is_none() {
                        self.list.append(group.root_widget());
                    }
                    self.list
                        .reorder_child_after(group.root_widget(), previous.as_ref());
                    previous = Some(group.root_widget().clone().upcast());
                    group.update(group_model, now);

                    let mut expanded_previous: Option<gtk::Widget> = None;
                    for notification in &group_model.notifications {
                        seen_rows.insert(notification.id);
                        let row = self
                            .rows
                            .entry(notification.id)
                            .or_insert_with(|| NotificationRow::new(sender));
                        row.update(notification, now, sender);
                        row.set_header_visible(false);

                        if group.is_expanded() {
                            row.set_card_actions_enabled(true);
                            place_row(row, group.expanded_list(), expanded_previous.as_ref());
                            expanded_previous = Some(row.root.as_ref().clone().upcast());
                        } else if notification.id == group_model.lead.id {
                            row.set_card_actions_enabled(false);
                            place_row(row, group.collapsed_stack(), None);
                        } else if let Some(parent) = row.root.as_ref().parent() {
                            if let Ok(parent) = parent.downcast::<gtk::Box>() {
                                parent.remove(row.root.as_ref());
                            }
                        }
                    }
                }
            }
        }

        self.rows.retain(|id, row| {
            let keep = seen_rows.contains(id);
            if !keep {
                if let Some(parent) = row.root.as_ref().parent() {
                    if let Ok(parent) = parent.downcast::<gtk::Box>() {
                        parent.remove(row.root.as_ref());
                    }
                }
            }
            keep
        });
        self.groups.retain(|key, group| {
            let keep = seen_groups.contains(key);
            if !keep {
                group.detach_rows();
                self.list.remove(group.root_widget());
            }
            keep
        });
    }

    fn refresh_times(&self) {
        let now = now_ms();
        for item in notification_items(&self.notifications) {
            match item {
                NotificationListItem::Notification(notification) => {
                    if let Some(row) = self.rows.get(&notification.id) {
                        row.update_time(&notification, now);
                    }
                }
                NotificationListItem::Group(group_model) => {
                    if let Some(group) = self.groups.get(&group_model.key) {
                        group.update_time(&group_model, now);
                    }
                    for notification in &group_model.notifications {
                        if let Some(row) = self.rows.get(&notification.id) {
                            row.update_time(notification, now);
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Popover {
    fn drop(&mut self) {
        if let Some(refresh_timer) = self.refresh_timer.take() {
            refresh_timer.remove();
        }
    }
}

fn place_row(row: &NotificationRow, container: &gtk::Box, previous: Option<&gtk::Widget>) {
    let row_widget = row.root.as_ref();
    let target = container.clone().upcast::<gtk::Widget>();
    let already_in_container = row_widget.parent().is_some_and(|parent| parent == target);

    if !already_in_container {
        if let Some(parent) = row_widget.parent() {
            if let Ok(parent) = parent.downcast::<gtk::Box>() {
                parent.remove(row_widget);
            }
        }
        container.append(row_widget);
    }
    container.reorder_child_after(row_widget, previous);
}

#[relm4::widget_template]
impl WidgetTemplate for NotificationRowView {
    view! {
        gtk::Box {
            add_css_class: "notification-card",
            add_css_class: "card-surface",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 4,

            #[name = "header"]
            gtk::Box {
                add_css_class: "card-surface__header",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                #[name = "icon"]
                gtk::Image {
                    add_css_class: "notification-card-icon",
                    add_css_class: "notification-icon",
                    set_valign: gtk::Align::Center,
                },

                #[name = "app_label"]
                gtk::Label {
                    add_css_class: "notification-app-name",
                    set_halign: gtk::Align::Start,
                },

                #[name = "time_label"]
                gtk::Label {
                    add_css_class: "notification-time",
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_hexpand: true,
                },

                #[name = "dismiss"]
                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "notification-dismiss",
                    set_icon_name: "window-close-symbolic",
                },
            },

            gtk::Box {
                add_css_class: "notification-content",
                add_css_class: "card-surface__body",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                set_hexpand: true,

                #[name = "image"]
                gtk::Picture {
                    add_css_class: "notification-inline-image",
                    set_can_shrink: true,
                    set_content_fit: gtk::ContentFit::Contain,
                    set_valign: gtk::Align::Start,
                    set_visible: false,
                },

                gtk::Box {
                    add_css_class: "notification-copy",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 4,
                    set_hexpand: true,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,

                        #[name = "summary_label"]
                        gtk::Label {
                            add_css_class: "notification-summary",
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_max_width_chars: 48,
                        },

                        #[name = "body_label"]
                        gtk::Label {
                            add_css_class: "notification-body",
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_max_width_chars: 55,
                            set_lines: 2,
                            set_wrap: true,
                            set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        },
                    },

                    #[name = "actions_box"]
                    gtk::Box {
                        add_css_class: "notification-actions",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 4,
                    },
                },
            },
        }
    }
}

struct NotificationRow {
    root: NotificationRowView,
    id: Rc<Cell<u32>>,
    card_actions_enabled: Rc<Cell<bool>>,
}

impl NotificationRow {
    fn new(sender: &ComponentSender<Popover>) -> Self {
        let root = NotificationRowView::init(());
        let id = Rc::new(Cell::new(0));
        let card_actions_enabled = Rc::new(Cell::new(true));

        root.dismiss.connect_clicked({
            let id = id.clone();
            let sender = sender.clone();
            move |_| sender.input(PopoverInput::Dismiss(id.get()))
        });

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        right_click.set_propagation_phase(gtk::PropagationPhase::Bubble);
        right_click.connect_pressed({
            let id = id.clone();
            let sender = sender.clone();
            let root_widget = root.as_ref().clone();
            let dismiss = root.dismiss.clone();
            let actions_box = root.actions_box.clone();
            let card_actions_enabled = card_actions_enabled.clone();
            move |gesture, _, x, y| {
                if !card_actions_enabled.get() {
                    return;
                }
                if point_inside_widget(&root_widget, &dismiss, x, y)
                    || point_inside_widget(&root_widget, &actions_box, x, y)
                {
                    return;
                }
                gesture.set_state(gtk::EventSequenceState::Claimed);
                sender.input(PopoverInput::Dismiss(id.get()));
            }
        });
        root.as_ref().add_controller(right_click);

        let root_click = gtk::GestureClick::new();
        root_click.set_button(1);
        root_click.set_propagation_phase(gtk::PropagationPhase::Bubble);
        root_click.connect_pressed({
            let id = id.clone();
            let sender = sender.clone();
            let root_widget = root.as_ref().clone();
            let dismiss = root.dismiss.clone();
            let actions_box = root.actions_box.clone();
            let card_actions_enabled = card_actions_enabled.clone();
            move |gesture, _, x, y| {
                if !card_actions_enabled.get() {
                    return;
                }
                if point_inside_widget(&root_widget, &dismiss, x, y)
                    || point_inside_widget(&root_widget, &actions_box, x, y)
                {
                    return;
                }
                gesture.set_state(gtk::EventSequenceState::Claimed);
                sender.input(PopoverInput::FocusAndDismiss(id.get()));
            }
        });
        root.as_ref().add_controller(root_click);

        Self {
            root,
            id,
            card_actions_enabled,
        }
    }

    fn set_card_actions_enabled(&self, enabled: bool) {
        self.card_actions_enabled.set(enabled);
        self.root.actions_box.set_sensitive(enabled);
        self.root
            .actions_box
            .set_visible(enabled && self.root.actions_box.first_child().is_some());
    }

    fn set_header_visible(&self, visible: bool) {
        self.root.header.set_visible(visible);
    }

    fn update(
        &self,
        notification: &NotificationEntry,
        now: u64,
        sender: &ComponentSender<Popover>,
    ) {
        self.id.set(notification.id);
        if notification.urgency == 2 {
            self.root
                .as_ref()
                .add_css_class("notification-card-critical");
        } else {
            self.root
                .as_ref()
                .remove_css_class("notification-card-critical");
        }

        self.root
            .icon
            .set_icon_name(Some(notification_icon_name(notification)));
        self.root
            .app_label
            .set_label(format::source_name(notification));
        self.root
            .time_label
            .set_label(&format::relative_time(now, notification.timestamp));

        if let Some(texture) = load_notification_image(notification) {
            self.root.image.set_paintable(Some(&texture));
            self.root.image.set_visible(true);
        } else {
            self.root.image.set_paintable(None::<&gdk::Texture>);
            self.root.image.set_visible(false);
        }

        self.root.summary_label.set_label(&notification.summary);
        self.root.body_label.set_label(&notification.body);
        self.root
            .body_label
            .set_visible(!notification.body.is_empty());

        clear_children(&self.root.actions_box);
        for (action_key, label) in format::visible_actions(notification) {
            let button = NotificationActionButton::init(NotificationActionButtonInit {
                label: label.into(),
                style: NotificationActionButtonStyle::Popover,
            });
            let id = notification.id;
            let action_key = action_key.to_string();
            let sender = sender.clone();
            button.as_ref().connect_clicked(move |_| {
                sender.input(PopoverInput::InvokeAction {
                    id,
                    action_key: action_key.clone(),
                });
            });
            self.root.actions_box.append(button.as_ref());
        }
        self.root
            .actions_box
            .set_visible(self.root.actions_box.first_child().is_some());
    }

    fn update_time(&self, notification: &NotificationEntry, now: u64) {
        self.root
            .time_label
            .set_label(&format::relative_time(now, notification.timestamp));
    }
}

fn notification_icon_name(notification: &NotificationEntry) -> &str {
    if notification.app_icon.is_empty() {
        "dialog-information-symbolic"
    } else {
        notification.app_icon.as_str()
    }
}

fn notification_popover_icon_name(dnd: bool) -> &'static str {
    format::icon_name(&NotificationState {
        dnd,
        ..Default::default()
    })
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

fn clear_children(container: &gtk::Box) {
    let mut child = container.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        container.remove(&widget);
    }
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
    fn notification_row_view_exposes_stable_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let row = NotificationRowView::init(());

        assert!(row.as_ref().has_css_class("notification-card"));
        assert!(row.as_ref().has_css_class("card-surface"));
        assert!(row.icon.has_css_class("notification-card-icon"));
        assert!(row.icon.has_css_class("notification-icon"));
        assert!(row.app_label.has_css_class("notification-app-name"));
        assert!(row.time_label.has_css_class("notification-time"));
        assert!(row.image.has_css_class("notification-inline-image"));
        assert!(row.summary_label.has_css_class("notification-summary"));
        assert!(row.body_label.has_css_class("notification-body"));
        assert!(row.actions_box.has_css_class("notification-actions"));
    }

    #[test]
    fn popover_uses_available_notification_icon_names() {
        assert_eq!(
            notification_popover_icon_name(false),
            "preferences-system-notifications-symbolic"
        );
        assert_eq!(
            notification_popover_icon_name(true),
            "notifications-disabled-symbolic"
        );
    }
}
