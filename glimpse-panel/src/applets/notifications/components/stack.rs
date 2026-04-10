use relm4::gtk::{self, prelude::*};

use super::row::{build_notification_row, resolve_notif_icon};
use super::{NotifData, NotificationCommandEmitter, StackToggleEmitter};
use crate::applets::notifications::NotificationActionCommand;

pub fn build_notification_group(
    app_name: &str,
    notifs: &[NotifData],
    stacked: bool,
    emit_command: NotificationCommandEmitter,
    on_toggle_stack: StackToggleEmitter,
) -> gtk::Widget {
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("notif-group");

    let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header_row.add_css_class("notif-group-header");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.set_hexpand(true);

    let icon = gtk::Image::from_icon_name(resolve_notif_icon(&notifs[0]));
    icon.set_pixel_size(16);
    icon.add_css_class("notif-icon");
    header.append(&icon);

    let app_label = gtk::Label::new(Some(&format!("{app_name} ({count})", count = notifs.len())));
    app_label.set_halign(gtk::Align::Start);
    app_label.set_hexpand(true);
    app_label.add_css_class("notif-app-name");
    header.append(&app_label);

    let header_btn = gtk::Button::new();
    header_btn.set_child(Some(&header));
    header_btn.set_hexpand(true);
    header_btn.add_css_class("flat");
    header_btn.add_css_class("notif-group-header-btn");
    let app = app_name.to_string();
    let toggle_header = on_toggle_stack.clone();
    header_btn.connect_clicked(move |_| {
        toggle_header(app.clone());
    });
    header_row.append(&header_btn);

    let clear_group = gtk::Button::from_icon_name("window-close-symbolic");
    apply_css_classes(&clear_group, group_dismiss_button_classes());
    clear_group.set_valign(gtk::Align::Center);
    clear_group.set_tooltip_text(Some("Dismiss group"));
    let dismiss_ids = group_dismiss_ids(notifs);
    let dismiss_group = emit_command.clone();
    clear_group.connect_clicked(move |_| {
        for id in &dismiss_ids {
            dismiss_group(NotificationActionCommand::Dismiss { id: *id });
        }
    });
    header_row.append(&clear_group);

    let chevron_icon = if stacked {
        "go-down-symbolic"
    } else {
        "go-up-symbolic"
    };
    let chevron = gtk::Button::from_icon_name(chevron_icon);
    apply_css_classes(&chevron, group_toggle_button_classes());
    let app = app_name.to_string();
    let toggle = on_toggle_stack.clone();
    chevron.connect_clicked(move |_| {
        toggle(app.clone());
    });
    header_row.append(&chevron);

    group.append(&header_row);

    let mut sorted: Vec<&NotifData> = notifs.iter().collect();
    sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    if stacked {
        let row = build_notification_row(sorted[0], emit_command);
        group.append(&row);

        if notifs.len() > 1 {
            let peek1 = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            peek1.add_css_class("notif-stack-depth");
            group.append(&peek1);
        }
        if notifs.len() > 2 {
            let peek2 = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            peek2.add_css_class("notif-stack-depth-2");
            group.append(&peek2);
        }
    } else {
        let cards_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        cards_box.add_css_class("notif-group-cards");
        for notif in sorted {
            let row = build_notification_row(notif, emit_command.clone());
            cards_box.append(&row);
        }
        group.append(&cards_box);
    }

    group.upcast()
}

fn group_dismiss_ids(notifs: &[NotifData]) -> Vec<u32> {
    notifs.iter().map(|notif| notif.id).collect()
}

fn apply_css_classes(button: &gtk::Button, classes: &[&str]) {
    for class in classes {
        button.add_css_class(class);
    }
}

fn group_dismiss_button_classes() -> &'static [&'static str] {
    &["flat", "notif-dismiss"]
}

fn group_toggle_button_classes() -> &'static [&'static str] {
    &["flat", "notif-expand-btn"]
}

#[cfg(test)]
mod tests {
    use super::{
        NotifData, group_dismiss_button_classes, group_dismiss_ids, group_toggle_button_classes,
    };

    fn notif(id: u32) -> NotifData {
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
            timestamp: 0,
            resident: false,
        }
    }

    #[test]
    fn group_dismiss_ids_collects_all_notification_ids() {
        assert_eq!(group_dismiss_ids(&[notif(4), notif(7), notif(9)]), vec![4, 7, 9]);
    }

    #[test]
    fn group_dismiss_button_reuses_notification_dismiss_style() {
        assert_eq!(group_dismiss_button_classes(), ["flat", "notif-dismiss"]);
    }

    #[test]
    fn group_toggle_button_keeps_expand_style() {
        assert_eq!(group_toggle_button_classes(), ["flat", "notif-expand-btn"]);
    }
}
