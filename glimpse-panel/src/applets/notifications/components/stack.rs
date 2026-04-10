use relm4::gtk::{self, prelude::*};

use super::row::{build_notification_row, resolve_notif_icon};
use super::{NotifData, NotificationCommandEmitter, StackToggleEmitter};

pub fn build_notification_group(
    app_name: &str,
    notifs: &[NotifData],
    stacked: bool,
    emit_command: NotificationCommandEmitter,
    on_toggle_stack: StackToggleEmitter,
) -> gtk::Widget {
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("notif-group");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("notif-group-header");

    let icon = gtk::Image::from_icon_name(resolve_notif_icon(&notifs[0]));
    icon.set_pixel_size(16);
    icon.add_css_class("notif-icon");
    header.append(&icon);

    let app_label = gtk::Label::new(Some(&format!("{app_name} ({count})", count = notifs.len())));
    app_label.set_halign(gtk::Align::Start);
    app_label.set_hexpand(true);
    app_label.add_css_class("notif-app-name");
    header.append(&app_label);

    let chevron_icon = if stacked {
        "go-down-symbolic"
    } else {
        "go-up-symbolic"
    };
    let chevron = gtk::Button::from_icon_name(chevron_icon);
    chevron.add_css_class("flat");
    chevron.add_css_class("notif-expand-btn");
    let app = app_name.to_string();
    let toggle = on_toggle_stack.clone();
    chevron.connect_clicked(move |_| {
        toggle(app.clone());
    });
    header.append(&chevron);

    let header_btn = gtk::Button::new();
    header_btn.set_child(Some(&header));
    header_btn.add_css_class("flat");
    header_btn.add_css_class("notif-group-header-btn");
    let app = app_name.to_string();
    header_btn.connect_clicked(move |_| {
        on_toggle_stack(app.clone());
    });
    group.append(&header_btn);

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
