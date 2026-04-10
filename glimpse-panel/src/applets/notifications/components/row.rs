use relm4::gtk::{self, glib, prelude::*};

use super::{NotifData, NotificationCommandEmitter};
use crate::applets::notifications::NotificationActionCommand;
use crate::applets::notifications::activation::{default_action_command, invoke_action_command};

pub fn resolve_notif_icon(notif: &NotifData) -> &str {
    if !notif.app_icon.is_empty() {
        return &notif.app_icon;
    }
    if let Some(ref de) = notif.desktop_entry {
        if !de.is_empty() {
            return de;
        }
    }
    "dialog-information-symbolic"
}

pub fn format_time_diff(diff_secs: u64) -> String {
    match diff_secs {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", diff_secs / 60),
        3600..=86399 => format!("{}h", diff_secs / 3600),
        86400..=172799 => "yesterday".into(),
        _ => format!("{}d", diff_secs / 86400),
    }
}

pub fn time_ago(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    format_time_diff(now.saturating_sub(timestamp) / 1000)
}

pub fn build_notification_row(
    notif: &NotifData,
    emit_command: NotificationCommandEmitter,
) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class("notif-card");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);

    let icon = gtk::Image::from_icon_name(resolve_notif_icon(notif));
    icon.set_pixel_size(20);
    icon.set_valign(gtk::Align::Center);
    icon.add_css_class("notif-icon");
    header.append(&icon);

    let app_name = if notif.app_name.is_empty() {
        "Notification"
    } else {
        &notif.app_name
    };
    let app_label = gtk::Label::new(Some(app_name));
    app_label.set_halign(gtk::Align::Start);
    app_label.set_hexpand(true);
    app_label.add_css_class("notif-app-name");
    header.append(&app_label);

    let time_label = gtk::Label::new(Some(&time_ago(notif.timestamp)));
    time_label.add_css_class("notif-time");
    header.append(&time_label);

    let dismiss = gtk::Button::from_icon_name("window-close-symbolic");
    dismiss.add_css_class("flat");
    dismiss.add_css_class("notif-dismiss");
    dismiss.set_valign(gtk::Align::Center);
    let id = notif.id;
    let dismiss_command = emit_command.clone();
    dismiss.connect_clicked(move |_| {
        dismiss_command(NotificationActionCommand::Dismiss { id });
    });
    header.append(&dismiss);

    card.append(&header);

    let summary = gtk::Label::new(Some(&notif.summary));
    summary.set_halign(gtk::Align::Start);
    summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
    summary.set_max_width_chars(40);
    summary.add_css_class("notif-summary");
    card.append(&summary);

    if !notif.body.is_empty() {
        let body = gtk::Label::new(Some(&notif.body));
        body.set_halign(gtk::Align::Start);
        body.set_ellipsize(gtk::pango::EllipsizeMode::End);
        body.set_max_width_chars(45);
        body.set_lines(2);
        body.set_wrap(true);
        body.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        body.add_css_class("notif-body");
        card.append(&body);
    }

    let visible_actions: Vec<&(String, String)> = notif
        .actions
        .iter()
        .filter(|(key, _)| key != "default")
        .collect();
    if !visible_actions.is_empty() {
        let actions_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        actions_box.add_css_class("notif-actions");
        for (key, label) in &visible_actions {
            let action_btn = gtk::Button::with_label(label);
            action_btn.add_css_class("flat");
            action_btn.add_css_class("notif-action-btn");
            let nid = notif.id;
            let k = key.clone();
            let action_command = emit_command.clone();
            action_btn.connect_clicked(move |_| {
                action_command(invoke_action_command(nid, &k, None));
            });
            actions_box.append(&action_btn);
        }
        card.append(&actions_box);
    }

    let has_default = notif.actions.iter().any(|(k, _)| k == "default");
    if has_default {
        let gesture = gtk::GestureClick::new();
        gesture.set_button(1);
        let id = notif.id;
        let desktop_entry = notif.desktop_entry.clone();
        let app_name = notif.app_name.clone();
        gesture.connect_pressed(move |g, _, _, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            let desktop_entry = desktop_entry.clone();
            let app_name = app_name.clone();
            let action_command = emit_command.clone();
            let timestamp = g.current_event_time();
            glib::spawn_future_local(async move {
                action_command(default_action_command(id, desktop_entry, app_name, timestamp).await);
            });
        });
        card.add_controller(gesture);
    }

    card.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_diff_now() {
        assert_eq!(format_time_diff(0), "now");
        assert_eq!(format_time_diff(30), "now");
        assert_eq!(format_time_diff(59), "now");
    }

    #[test]
    fn time_diff_minutes() {
        assert_eq!(format_time_diff(60), "1m");
        assert_eq!(format_time_diff(120), "2m");
        assert_eq!(format_time_diff(3599), "59m");
    }

    #[test]
    fn time_diff_hours() {
        assert_eq!(format_time_diff(3600), "1h");
        assert_eq!(format_time_diff(7200), "2h");
        assert_eq!(format_time_diff(86399), "23h");
    }

    #[test]
    fn time_diff_yesterday() {
        assert_eq!(format_time_diff(86400), "yesterday");
        assert_eq!(format_time_diff(172799), "yesterday");
    }

    #[test]
    fn time_diff_days() {
        assert_eq!(format_time_diff(172800), "2d");
        assert_eq!(format_time_diff(604800), "7d");
    }
}
