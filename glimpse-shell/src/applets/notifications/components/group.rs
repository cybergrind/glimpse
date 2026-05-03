#![allow(unused_assignments)]

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

use crate::services::notifications::model::NotificationEntry;

use super::super::format;

const GROUP_THRESHOLD: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NotificationListItem {
    Group(NotificationGroupModel),
    Notification(NotificationEntry),
}

impl NotificationListItem {
    pub fn timestamp(&self) -> u64 {
        match self {
            Self::Group(group) => group.lead.timestamp,
            Self::Notification(notification) => notification.timestamp,
        }
    }

    pub fn id(&self) -> u32 {
        match self {
            Self::Group(group) => group.lead.id,
            Self::Notification(notification) => notification.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NotificationGroupModel {
    pub key: String,
    pub app_name: String,
    pub icon: String,
    pub ids: Vec<u32>,
    pub lead: NotificationEntry,
    pub notifications: Vec<NotificationEntry>,
}

pub(crate) enum NotificationGroupAction {
    Dismiss(Vec<u32>),
    Toggle(String),
}

#[relm4::widget_template(pub(crate))]
impl WidgetTemplate for NotificationGroupView {
    view! {
        gtk::Box {
            add_css_class: "notif-group",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 7,

            gtk::Box {
                add_css_class: "notif-group-header",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                #[name = "icon"]
                gtk::Image {
                    add_css_class: "notif-header-icon",
                    set_pixel_size: 16,
                    set_valign: gtk::Align::Center,
                },

                #[name = "title"]
                gtk::Label {
                    add_css_class: "notif-group-title",
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                },

                #[name = "count"]
                gtk::Label {
                    add_css_class: "badge",
                    set_visible: false,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_hexpand: true,
                },

                #[name = "time"]
                gtk::Label {
                    add_css_class: "notif-time",
                },

                #[name = "dismiss"]
                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "notif-dismiss",
                    set_icon_name: "window-close-symbolic",
                },

                #[name = "toggle"]
                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "notif-expand-btn",
                    set_visible: false,
                },
            },

            #[name = "stack"]
            gtk::Box {
                add_css_class: "notif-stack-backplates",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,

                #[name = "collapsed_lead"]
                gtk::Box {
                    add_css_class: "notif-group-lead",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 0,
                },

                #[name = "second_backplate"]
                gtk::Box {
                    add_css_class: "notif-stack-backplate",
                    add_css_class: "notif-stack-backplate-second",
                    set_visible: false,
                },

                #[name = "lower_backplate"]
                gtk::Box {
                    add_css_class: "notif-stack-backplate",
                    add_css_class: "notif-stack-backplate-lower",
                    set_visible: false,
                },
            },

            #[name = "expanded_list"]
            gtk::Box {
                add_css_class: "notif-group-cards",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 6,
                set_visible: false,
            },
        }
    }
}

pub(crate) struct NotificationGroup {
    root: NotificationGroupView,
    ids: Rc<RefCell<Vec<u32>>>,
    expanded: Rc<Cell<bool>>,
}

impl NotificationGroup {
    pub fn new<F>(key: &str, emit: F) -> Self
    where
        F: Fn(NotificationGroupAction) + 'static,
    {
        let root = NotificationGroupView::init(());
        let ids = Rc::new(RefCell::new(Vec::new()));
        let expanded = Rc::new(Cell::new(false));
        let emit = Rc::new(emit);

        root.dismiss.connect_clicked({
            let ids = ids.clone();
            let emit = emit.clone();
            move |_| emit(NotificationGroupAction::Dismiss(ids.borrow().clone()))
        });

        root.toggle.connect_clicked({
            let key = key.to_owned();
            let emit = emit.clone();
            move |_| emit(NotificationGroupAction::Toggle(key.clone()))
        });

        let stack_click = gtk::GestureClick::new();
        stack_click.set_button(1);
        stack_click.set_propagation_phase(gtk::PropagationPhase::Bubble);
        stack_click.connect_pressed({
            let key = key.to_owned();
            let ids = ids.clone();
            let emit = emit.clone();
            move |gesture, _, _, _| {
                if ids.borrow().len() <= 1 {
                    return;
                }
                gesture.set_state(gtk::EventSequenceState::Claimed);
                emit(NotificationGroupAction::Toggle(key.clone()));
            }
        });
        root.stack.add_controller(stack_click);

        Self {
            root,
            ids,
            expanded,
        }
    }

    pub fn root_widget(&self) -> &gtk::Box {
        self.root.as_ref()
    }

    pub fn collapsed_stack(&self) -> &gtk::Box {
        &self.root.collapsed_lead
    }

    pub fn expanded_list(&self) -> &gtk::Box {
        &self.root.expanded_list
    }

    pub fn detach_rows(&self) {
        remove_children(&self.root.collapsed_lead);
        remove_children(&self.root.expanded_list);
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    pub fn update(&self, group: &NotificationGroupModel, now: u64) {
        self.ids.replace(group.ids.clone());
        self.root.icon.set_icon_name(Some(&group.icon));
        self.root.title.set_label(&group.app_name);
        self.root.dismiss.set_tooltip_text(Some("Dismiss group"));
        self.update_time(group, now);

        let count = group.notifications.len();
        self.root.count.set_label(&count.to_string());
        self.root.count.set_visible(count > 1);
        self.root.toggle.set_visible(count > 1);
        if count <= 1 {
            self.expanded.set(false);
        }
        self.apply_expanded_state(count);
    }

    pub fn update_time(&self, group: &NotificationGroupModel, now: u64) {
        self.root
            .time
            .set_label(&format::relative_time(now, group.lead.timestamp));
    }

    pub fn toggle(&self) {
        self.expanded.set(!self.expanded.get());
        let count = self.ids.borrow().len();
        self.apply_expanded_state(count);
    }

    fn apply_expanded_state(&self, count: usize) {
        let expanded = self.expanded.get() && count > 1;
        self.root.stack.set_visible(!expanded);
        self.root.expanded_list.set_visible(expanded);
        self.root
            .second_backplate
            .set_visible(!expanded && count > 1);
        self.root
            .lower_backplate
            .set_visible(!expanded && count > 2);
        self.root.toggle.set_icon_name(if expanded {
            "pan-up-symbolic"
        } else {
            "pan-down-symbolic"
        });

        if expanded {
            self.root.as_ref().add_css_class("notif-group-expanded");
            self.root.as_ref().remove_css_class("notif-group-collapsed");
        } else {
            self.root.as_ref().remove_css_class("notif-group-expanded");
            self.root.as_ref().add_css_class("notif-group-collapsed");
        }
    }
}

pub(crate) fn notification_items(notifications: &[NotificationEntry]) -> Vec<NotificationListItem> {
    let mut items = Vec::new();
    for group in group_notifications(notifications) {
        if group.notifications.len() >= GROUP_THRESHOLD {
            items.push(NotificationListItem::Group(group));
        } else {
            items.extend(
                group
                    .notifications
                    .into_iter()
                    .map(NotificationListItem::Notification),
            );
        }
    }

    items.sort_by(|left, right| {
        right
            .timestamp()
            .cmp(&left.timestamp())
            .then_with(|| right.id().cmp(&left.id()))
    });
    items
}

fn group_notifications(notifications: &[NotificationEntry]) -> Vec<NotificationGroupModel> {
    let mut by_key = HashMap::<String, Vec<NotificationEntry>>::new();
    for notification in notifications {
        by_key
            .entry(notification_group_key(notification))
            .or_default()
            .push(notification.clone());
    }

    let mut groups = by_key
        .into_iter()
        .filter_map(|(key, mut notifications)| {
            notifications.sort_by(|left, right| {
                right
                    .timestamp
                    .cmp(&left.timestamp)
                    .then_with(|| right.id.cmp(&left.id))
            });
            let lead = notifications.first()?.clone();
            let app_name = format::source_name(&lead).to_owned();
            let icon = notification_icon_name(&lead).to_owned();
            let ids = notifications
                .iter()
                .map(|notification| notification.id)
                .collect::<Vec<_>>();

            Some(NotificationGroupModel {
                key,
                app_name,
                icon,
                ids,
                lead,
                notifications,
            })
        })
        .collect::<Vec<_>>();

    groups.sort_by(|left, right| {
        right
            .lead
            .timestamp
            .cmp(&left.lead.timestamp)
            .then_with(|| right.lead.id.cmp(&left.lead.id))
            .then_with(|| left.app_name.cmp(&right.app_name))
    });
    groups
}

fn notification_group_key(notification: &NotificationEntry) -> String {
    notification
        .desktop_entry
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let app_name = notification.app_name.trim();
            (!app_name.is_empty()).then_some(app_name)
        })
        .unwrap_or("notification")
        .to_lowercase()
}

fn notification_icon_name(notification: &NotificationEntry) -> &str {
    if notification.app_icon.is_empty() {
        "dialog-information-symbolic"
    } else {
        notification.app_icon.as_str()
    }
}

fn remove_children(container: &gtk::Box) {
    let mut child = container.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        container.remove(&widget);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn notification_group_view_exposes_stable_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let group = NotificationGroupView::init(());

        assert!(group.as_ref().has_css_class("notif-group"));
        assert!(group.icon.has_css_class("notif-header-icon"));
        assert!(group.title.has_css_class("notif-group-title"));
        assert!(group.count.has_css_class("badge"));
        assert!(group.time.has_css_class("notif-time"));
        assert!(group.dismiss.has_css_class("notif-dismiss"));
        assert!(group.toggle.has_css_class("notif-expand-btn"));
        assert!(group.stack.has_css_class("notif-stack-backplates"));
        assert!(group.collapsed_lead.has_css_class("notif-group-lead"));
        assert!(
            group
                .second_backplate
                .has_css_class("notif-stack-backplate")
        );
        assert!(
            group
                .second_backplate
                .has_css_class("notif-stack-backplate-second")
        );
        assert!(group.lower_backplate.has_css_class("notif-stack-backplate"));
        assert!(
            group
                .lower_backplate
                .has_css_class("notif-stack-backplate-lower")
        );
        assert!(group.expanded_list.has_css_class("notif-group-cards"));
    }

    #[test]
    fn group_notifications_uses_desktop_entry_and_sorts_by_latest() {
        let groups = group_notifications(&[
            notification(1, "Telegram", Some("org.telegram.desktop"), 100),
            notification(2, "Telegram Desktop", Some("org.telegram.desktop"), 300),
            notification(3, "Mail", Some("org.gnome.Geary"), 200),
        ]);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].key, "org.telegram.desktop");
        assert_eq!(groups[0].ids, vec![2, 1]);
        assert_eq!(groups[0].lead.id, 2);
        assert_eq!(groups[1].key, "org.gnome.geary");
        assert_eq!(groups[1].ids, vec![3]);
    }

    #[test]
    fn group_notifications_falls_back_to_app_name() {
        let groups = group_notifications(&[
            notification(1, "Telegram", None, 100),
            notification(2, "telegram", None, 200),
        ]);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key, "telegram");
        assert_eq!(groups[0].ids, vec![2, 1]);
    }

    #[test]
    fn notification_items_keep_two_notifications_ungrouped() {
        let items = notification_items(&[
            notification(1, "Telegram", Some("org.telegram.desktop"), 100),
            notification(2, "Telegram", Some("org.telegram.desktop"), 200),
        ]);

        assert!(matches!(&items[0], NotificationListItem::Notification(item) if item.id == 2));
        assert!(matches!(&items[1], NotificationListItem::Notification(item) if item.id == 1));
    }

    #[test]
    fn notification_items_group_three_notifications() {
        let items = notification_items(&[
            notification(1, "Telegram", Some("org.telegram.desktop"), 100),
            notification(2, "Telegram", Some("org.telegram.desktop"), 200),
            notification(3, "Telegram", Some("org.telegram.desktop"), 300),
        ]);

        assert_eq!(items.len(), 1);
        assert!(
            matches!(&items[0], NotificationListItem::Group(group) if group.ids == vec![3, 2, 1])
        );
    }

    fn notification(
        id: u32,
        app_name: &str,
        desktop_entry: Option<&str>,
        timestamp: u64,
    ) -> NotificationEntry {
        NotificationEntry {
            id,
            app_name: app_name.into(),
            app_icon: "dialog-information-symbolic".into(),
            desktop_entry: desktop_entry.map(str::to_owned),
            summary: format!("Summary {id}"),
            body: String::new(),
            urgency: 1,
            actions: Vec::new(),
            image: None,
            timestamp,
            resident: false,
        }
    }
}
