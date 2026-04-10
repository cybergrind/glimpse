use std::collections::{HashMap, HashSet};

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::row::{NotificationCard, NotificationCardInit, NotificationCardInput};
use super::stack::{NotificationGroup, NotificationGroupInit, NotificationGroupInput};
use super::{NotifData, NotificationCommandEmitter, StackToggleEmitter};

pub struct NotificationsList {
    empty_label: gtk::Label,
    notif_box: gtk::Box,
    emit_command: NotificationCommandEmitter,
    on_toggle_stack: StackToggleEmitter,
    groups: HashMap<String, Controller<NotificationGroup>>,
    singles: HashMap<u32, Controller<NotificationCard>>,
}

pub struct NotificationsListInit {
    pub emit_command: NotificationCommandEmitter,
    pub on_toggle_stack: StackToggleEmitter,
}

#[derive(Debug)]
pub enum NotificationsListInput {
    Sync {
        notifications: Vec<NotifData>,
        stack_state: HashMap<String, bool>,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for NotificationsList {
    type Init = NotificationsListInit;
    type Input = NotificationsListInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,

            #[name(empty_label)]
            gtk::Label {
                set_label: "No notifications",
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                add_css_class: "notif-empty",
            },

            #[name(scroll)]
            gtk::ScrolledWindow {
                set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                set_max_content_height: 600,
                set_propagate_natural_height: true,
                set_vexpand: true,

                #[name(notif_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 4,
                    add_css_class: "notif-list",
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        _init_root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();

        let model = NotificationsList {
            empty_label: widgets.empty_label.clone(),
            notif_box: widgets.notif_box.clone(),
            emit_command: init.emit_command,
            on_toggle_stack: init.on_toggle_stack,
            groups: HashMap::new(),
            singles: HashMap::new(),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let NotificationsListInput::Sync {
            notifications,
            stack_state,
        } = msg;

        self.sync(&notifications, &stack_state);
    }
}

impl NotificationsList {
    fn sync(&mut self, notifications: &[NotifData], stack_state: &HashMap<String, bool>) {
        clear_children(&self.notif_box);
        self.empty_label.set_visible(notifications.is_empty());

        if notifications.is_empty() {
            return;
        }

        let mut seen_groups = HashSet::new();
        let mut seen_singles = HashSet::new();

        for (app_name, group) in grouped_notifications(notifications) {
            if group.len() > 1 {
                seen_groups.insert(app_name.clone());
                let stacked = *stack_state.get(&app_name).unwrap_or(&true);
                let ctrl = self.groups.entry(app_name.clone()).or_insert_with(|| {
                    NotificationGroup::builder()
                        .launch(NotificationGroupInit {
                            app_name: app_name.clone(),
                            notifications: group.clone(),
                            stacked,
                            emit_command: self.emit_command.clone(),
                            on_toggle_stack: self.on_toggle_stack.clone(),
                        })
                        .detach()
                });
                ctrl.emit(NotificationGroupInput::Update {
                    app_name: app_name.clone(),
                    notifications: group.clone(),
                    stacked,
                });
                self.notif_box.append(ctrl.widget());
            } else {
                let notif = group[0].clone();
                seen_singles.insert(notif.id);
                let ctrl = self.singles.entry(notif.id).or_insert_with(|| {
                    NotificationCard::builder()
                        .launch(NotificationCardInit {
                            notif: notif.clone(),
                            emit_command: self.emit_command.clone(),
                        })
                        .detach()
                });
                ctrl.emit(NotificationCardInput::Update(notif));
                self.notif_box.append(ctrl.widget());
            }
        }

        self.groups.retain(|name, _| seen_groups.contains(name));
        self.singles.retain(|id, _| seen_singles.contains(id));
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
