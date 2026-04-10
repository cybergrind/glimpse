use std::collections::{HashMap, HashSet};

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::group_header::{GroupHeader, GroupHeaderInit, GroupHeaderInput};
use super::row::{NotificationCard, NotificationCardInit, NotificationCardInput, NotificationCardRole};
use super::stack_hint::{StackHint, StackHintInit, StackHintInput};
use super::{NotifData, NotificationCommandEmitter, StackToggleEmitter};

pub struct NotificationGroup {
    root: gtk::Box,
    app_name: String,
    notifications: Vec<NotifData>,
    stacked: bool,
    emit_command: NotificationCommandEmitter,
    header: Controller<GroupHeader>,
    preview_top: Controller<NotificationCard>,
    hint: Controller<StackHint>,
    collapsed_body: gtk::Overlay,
    cards_box: gtk::Box,
    expanded_cards: HashMap<u32, Controller<NotificationCard>>,
}

pub struct NotificationGroupInit {
    pub app_name: String,
    pub notifications: Vec<NotifData>,
    pub stacked: bool,
    pub emit_command: NotificationCommandEmitter,
    pub on_toggle_stack: StackToggleEmitter,
}

#[derive(Debug)]
pub enum NotificationGroupInput {
    Update {
        app_name: String,
        notifications: Vec<NotifData>,
        stacked: bool,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for NotificationGroup {
    type Init = NotificationGroupInit;
    type Input = NotificationGroupInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "notif-group",

            #[local_ref]
            header_widget -> gtk::Box {},

            #[name(collapsed_body)]
            gtk::Overlay {
                set_child: Some(&hint_widget),
                add_overlay: &top_widget,
                add_css_class: "notif-group-collapsed-body",
            },

            #[name(cards_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 6,
                add_css_class: "notif-group-cards",
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let sorted = sorted_group_notifications(&init.notifications);
        let preview = collapsed_preview_notifications(&init.notifications);
        let hint_depth = preview.len().saturating_sub(1).min(2);

        let header = GroupHeader::builder()
            .launch(GroupHeaderInit {
                app_name: init.app_name.clone(),
                newest: sorted[0].clone(),
                stacked: init.stacked,
                dismiss_ids: group_dismiss_ids(&init.notifications),
                emit_command: init.emit_command.clone(),
                on_toggle_stack: init.on_toggle_stack.clone(),
            })
            .detach();
        let header_widget = header.widget().clone();

        let hint = StackHint::builder()
            .launch(StackHintInit { depth: hint_depth })
            .detach();
        let hint_widget = hint.widget().clone();

        let preview_top = NotificationCard::builder()
            .launch(NotificationCardInit {
                notif: preview[0].clone(),
                emit_command: init.emit_command.clone(),
                role: NotificationCardRole::Full,
            })
            .detach();
        preview_top.widget().add_css_class("notif-group-lead");
        preview_top.widget().set_halign(gtk::Align::Fill);
        preview_top.widget().set_valign(gtk::Align::Start);
        let top_widget = preview_top.widget().clone();

        let widgets = view_output!();
        widgets
            .collapsed_body
            .set_measure_overlay(&top_widget, true);

        let mut model = NotificationGroup {
            root: widgets.root.clone(),
            app_name: init.app_name,
            notifications: init.notifications,
            stacked: init.stacked,
            emit_command: init.emit_command,
            header,
            preview_top,
            hint,
            collapsed_body: widgets.collapsed_body.clone(),
            cards_box: widgets.cards_box.clone(),
            expanded_cards: HashMap::new(),
        };
        model.refresh();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let NotificationGroupInput::Update {
            app_name,
            notifications,
            stacked,
        } = msg;

        self.app_name = app_name;
        self.notifications = notifications;
        self.stacked = stacked;
        self.refresh();
    }
}

impl NotificationGroup {
    fn refresh(&mut self) {
        self.root.remove_css_class("notif-group-collapsed");
        self.root.remove_css_class("notif-group-expanded");
        self.root.add_css_class(if self.stacked {
            "notif-group-collapsed"
        } else {
            "notif-group-expanded"
        });

        let sorted = sorted_group_notifications(&self.notifications);
        if sorted.is_empty() {
            self.collapsed_body.set_visible(false);
            self.cards_box.set_visible(false);
            return;
        }

        self.header.emit(GroupHeaderInput::Update {
            app_name: self.app_name.clone(),
            newest: sorted[0].clone(),
            stacked: self.stacked,
            dismiss_ids: group_dismiss_ids(&self.notifications),
        });

        let preview = collapsed_preview_notifications(&self.notifications);
        let hint_depth = preview.len().saturating_sub(1).min(2);
        self.hint.emit(StackHintInput::SetDepth(hint_depth));
        self.preview_top
            .emit(NotificationCardInput::Update(preview[0].clone()));
        self.preview_top
            .emit(NotificationCardInput::SetRole(NotificationCardRole::Full));
        self.preview_top.widget().set_visible(self.stacked);
        self.preview_top.widget().set_can_target(self.stacked);

        self.collapsed_body.set_visible(self.stacked);

        self.cards_box.set_visible(!self.stacked);
        clear_children(&self.cards_box);

        let mut seen = HashSet::new();
        for notif in sorted {
            seen.insert(notif.id);
            let ctrl = self.expanded_cards.entry(notif.id).or_insert_with(|| {
                NotificationCard::builder()
                    .launch(NotificationCardInit {
                        notif: notif.clone(),
                        emit_command: self.emit_command.clone(),
                        role: NotificationCardRole::Full,
                    })
                    .detach()
            });
            ctrl.emit(NotificationCardInput::Update(notif.clone()));
            ctrl.emit(NotificationCardInput::SetRole(NotificationCardRole::Full));
            if !self.stacked {
                self.cards_box.append(ctrl.widget());
            }
        }

        self.expanded_cards.retain(|id, _| seen.contains(id));
    }
}

fn clear_children(container: &gtk::Box) {
    let mut child = container.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        container.remove(&widget);
    }
}

fn sorted_group_notifications(notifs: &[NotifData]) -> Vec<NotifData> {
    let mut sorted = notifs.to_vec();
    sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    sorted
}

fn collapsed_preview_notifications(notifs: &[NotifData]) -> Vec<NotifData> {
    let mut sorted = sorted_group_notifications(notifs);
    sorted.truncate(3);
    sorted
}

fn group_dismiss_ids(notifs: &[NotifData]) -> Vec<u32> {
    notifs.iter().map(|notif| notif.id).collect()
}

#[cfg(test)]
mod tests {
    use super::{NotifData, collapsed_preview_notifications, group_dismiss_ids};

    fn notif(id: u32) -> NotifData {
        notif_with_ts(id, 0)
    }

    fn notif_with_ts(id: u32, timestamp: u64) -> NotifData {
        NotifData {
            id,
            app_name: "Mail".to_string(),
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
    fn group_dismiss_ids_collects_all_notification_ids() {
        assert_eq!(
            group_dismiss_ids(&[notif(4), notif(7), notif(9)]),
            vec![4, 7, 9]
        );
    }

    #[test]
    fn collapsed_preview_notifications_put_newest_first() {
        let sorted: Vec<u32> = collapsed_preview_notifications(&[
            notif_with_ts(1, 100),
            notif_with_ts(2, 300),
            notif_with_ts(3, 200),
        ])
        .into_iter()
        .map(|notif| notif.id)
        .collect();

        assert_eq!(sorted, vec![2, 3, 1]);
    }

    #[test]
    fn collapsed_preview_notifications_cap_at_three_cards() {
        let sorted: Vec<u32> = collapsed_preview_notifications(&[
            notif_with_ts(1, 100),
            notif_with_ts(2, 400),
            notif_with_ts(3, 300),
            notif_with_ts(4, 200),
        ])
        .into_iter()
        .map(|notif| notif.id)
        .collect();

        assert_eq!(sorted, vec![2, 3, 4]);
    }
}
