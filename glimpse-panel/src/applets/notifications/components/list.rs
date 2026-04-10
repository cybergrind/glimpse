use std::collections::HashMap;

use relm4::gtk::{self, prelude::*};

use super::row::build_notification_row;
use super::stack::build_notification_group;
use super::{NotifData, NotificationCommandEmitter, StackToggleEmitter};

pub struct NotificationsList {
    root: gtk::Box,
    empty_label: gtk::Label,
    notif_box: gtk::Box,
}

impl NotificationsList {
    pub fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let empty_label = gtk::Label::new(Some("No notifications"));
        empty_label.set_halign(gtk::Align::Center);
        empty_label.set_valign(gtk::Align::Center);
        empty_label.add_css_class("notif-empty");
        root.append(&empty_label);

        let notif_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        notif_box.add_css_class("notif-list");

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_max_content_height(600);
        scroll.set_propagate_natural_height(true);
        scroll.set_vexpand(true);
        scroll.set_child(Some(&notif_box));
        root.append(&scroll);

        Self {
            root,
            empty_label,
            notif_box,
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn rebuild(
        &self,
        notifications: &[NotifData],
        stack_state: &HashMap<String, bool>,
        emit_command: NotificationCommandEmitter,
        on_toggle_stack: StackToggleEmitter,
    ) {
        clear_children(&self.notif_box);
        self.empty_label.set_visible(notifications.is_empty());

        if notifications.is_empty() {
            return;
        }

        for (app_name, group) in grouped_notifications(notifications) {
            if group.len() > 1 {
                let stacked = *stack_state.get(&app_name).unwrap_or(&true);
                let widget = build_notification_group(
                    &app_name,
                    &group,
                    stacked,
                    emit_command.clone(),
                    on_toggle_stack.clone(),
                );
                self.notif_box.append(&widget);
            } else {
                let row = build_notification_row(&group[0], emit_command.clone());
                self.notif_box.append(&row);
            }
        }
    }
}

fn clear_children(container: &gtk::Box) {
    let mut child = container.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        container.remove(&widget);
    }
}

fn grouped_notifications(notifications: &[NotifData]) -> Vec<(String, Vec<NotifData>)> {
    let mut groups: Vec<(String, Vec<NotifData>)> = Vec::new();
    let mut group_map: HashMap<String, usize> = HashMap::new();

    for notif in notifications {
        let key = if notif.app_name.is_empty() {
            "Unknown".to_string()
        } else {
            notif.app_name.clone()
        };

        if let Some(&idx) = group_map.get(&key) {
            groups[idx].1.push(notif.clone());
        } else {
            let idx = groups.len();
            group_map.insert(key.clone(), idx);
            groups.push((key, vec![notif.clone()]));
        }
    }

    groups.sort_by(|a, b| {
        let a_ts = a.1.iter().map(|n| n.timestamp).max().unwrap_or(0);
        let b_ts = b.1.iter().map(|n| n.timestamp).max().unwrap_or(0);
        b_ts.cmp(&a_ts)
    });

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notif(id: u32, app_name: &str, timestamp: u64) -> NotifData {
        NotifData {
            id,
            app_name: app_name.to_string(),
            app_icon: String::new(),
            desktop_entry: None,
            summary: String::new(),
            body: String::new(),
            urgency: 0,
            actions: Vec::new(),
            image: None,
            timestamp,
            resident: false,
        }
    }

    #[test]
    fn groups_notifications_by_app_and_orders_newest_first() {
        let grouped = grouped_notifications(&[
            notif(1, "Mail", 100),
            notif(2, "Chat", 300),
            notif(3, "Mail", 200),
        ]);

        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[0].0, "Chat");
        assert_eq!(grouped[1].0, "Mail");
        assert_eq!(grouped[1].1.len(), 2);
    }

    #[test]
    fn groups_empty_app_names_as_unknown() {
        let grouped = grouped_notifications(&[notif(1, "", 100)]);

        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].0, "Unknown");
    }
}
