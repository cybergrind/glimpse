use adw::prelude::*;
use relm4::gtk;

use crate::services::session::SessionAction;

use super::Config;

const DIALOG_WIDTH: i32 = 420;
const ACCEPT_BUTTON_CLASS: &str = "suggested-action";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfirmationSpec {
    pub heading: &'static str,
    pub body: &'static str,
    pub accept_label: &'static str,
    pub icon_name: &'static str,
}

pub fn confirmation_spec(action: SessionAction, config: &Config) -> Option<ConfirmationSpec> {
    match action {
        SessionAction::Lock => None,
        SessionAction::Logout if config.confirm_logout => Some(ConfirmationSpec {
            heading: "Log Out",
            body: "End the current session? Open apps will be closed.",
            accept_label: "Log Out",
            icon_name: "system-log-out-symbolic",
        }),
        SessionAction::Suspend if config.confirm_suspend => Some(ConfirmationSpec {
            heading: "Suspend",
            body: "Suspend this computer now?",
            accept_label: "Suspend",
            icon_name: "media-playback-pause-symbolic",
        }),
        SessionAction::Hibernate if config.confirm_hibernate => Some(ConfirmationSpec {
            heading: "Hibernate",
            body: "Hibernate this computer now?",
            accept_label: "Hibernate",
            icon_name: "document-save-symbolic",
        }),
        SessionAction::Reboot if config.confirm_reboot => Some(ConfirmationSpec {
            heading: "Restart",
            body: "Restart this computer? All users will be signed out.",
            accept_label: "Restart",
            icon_name: "system-reboot-symbolic",
        }),
        SessionAction::PowerOff if config.confirm_shutdown => Some(ConfirmationSpec {
            heading: "Shut Down",
            body: "Shut down this computer? All users will be signed out.",
            accept_label: "Shut Down",
            icon_name: "system-shutdown-symbolic",
        }),
        _ => None,
    }
}

pub fn show_confirmation(
    parent: &impl IsA<gtk::Widget>,
    spec: ConfirmationSpec,
    on_accept: impl Fn() + 'static,
) {
    let dialog = adw::Dialog::new();
    dialog.add_css_class("session-confirmation-dialog");
    dialog.set_content_width(DIALOG_WIDTH);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.add_css_class("session-confirmation-dialog__content");

    let message = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    message.add_css_class("session-confirmation-dialog__message");
    message.append(&icon_frame(&spec));
    message.append(&message_text(&spec));

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.add_css_class("session-confirmation-dialog__actions");
    actions.set_halign(gtk::Align::End);

    let cancel_button = gtk::Button::with_label("Cancel");
    cancel_button.add_css_class("session-confirmation-dialog__button");
    {
        let dialog = dialog.clone();
        cancel_button.connect_clicked(move |_| {
            dialog.close();
        });
    }

    let accept_button = gtk::Button::with_label(spec.accept_label);
    accept_button.add_css_class("session-confirmation-dialog__button");
    accept_button.add_css_class(ACCEPT_BUTTON_CLASS);
    {
        let dialog = dialog.clone();
        accept_button.connect_clicked(move |_| {
            on_accept();
            dialog.close();
        });
    }

    actions.append(&cancel_button);
    actions.append(&accept_button);

    content.append(&message);
    content.append(&actions);

    dialog.set_child(Some(&content));
    dialog.present(Some(parent));
}

fn icon_frame(spec: &ConfirmationSpec) -> gtk::Box {
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.add_css_class("session-confirmation-dialog__icon-frame");
    frame.set_valign(gtk::Align::Start);
    frame.append(&icon(spec));
    frame
}

fn icon(spec: &ConfirmationSpec) -> gtk::Image {
    let icon = gtk::Image::from_icon_name(spec.icon_name);
    icon.add_css_class("session-confirmation-dialog__icon");
    icon.set_pixel_size(36);
    icon
}

fn message_text(spec: &ConfirmationSpec) -> gtk::Box {
    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text.add_css_class("session-confirmation-dialog__text");
    text.set_hexpand(true);

    let heading = gtk::Label::new(Some(spec.heading));
    heading.add_css_class("session-confirmation-dialog__heading");
    heading.set_halign(gtk::Align::Start);
    heading.set_xalign(0.0);
    heading.set_wrap(true);

    let body = gtk::Label::new(Some(spec.body));
    body.add_css_class("session-confirmation-dialog__body");
    body.set_halign(gtk::Align::Start);
    body.set_xalign(0.0);
    body.set_wrap(true);

    text.append(&heading);
    text.append(&body);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_does_not_need_confirmation() {
        assert!(confirmation_spec(SessionAction::Lock, &Config::default()).is_none());
    }

    #[test]
    fn session_ending_actions_have_specific_confirmation_copy() {
        let config = Config::default();

        let logout = confirmation_spec(SessionAction::Logout, &config).unwrap();
        assert_eq!(logout.heading, "Log Out");
        assert!(logout.body.contains("Open apps"));
        assert_eq!(logout.icon_name, "system-log-out-symbolic");

        let reboot = confirmation_spec(SessionAction::Reboot, &config).unwrap();
        assert_eq!(reboot.heading, "Restart");
        assert!(reboot.body.contains("All users"));
        assert_eq!(reboot.icon_name, "system-reboot-symbolic");

        let shutdown = confirmation_spec(SessionAction::PowerOff, &config).unwrap();
        assert_eq!(shutdown.heading, "Shut Down");
        assert!(shutdown.body.contains("All users"));
        assert_eq!(shutdown.icon_name, "system-shutdown-symbolic");
    }

    #[test]
    fn accept_button_uses_suggested_style_only() {
        assert_eq!(ACCEPT_BUTTON_CLASS, "suggested-action");
        assert_ne!(ACCEPT_BUTTON_CLASS, "destructive-action");
    }

    #[test]
    fn confirmation_flags_are_respected() {
        let config = Config {
            confirm_logout: false,
            confirm_suspend: false,
            confirm_hibernate: false,
            confirm_reboot: false,
            confirm_shutdown: false,
            ..Config::default()
        };

        assert!(confirmation_spec(SessionAction::Logout, &config).is_none());
        assert!(confirmation_spec(SessionAction::Suspend, &config).is_none());
        assert!(confirmation_spec(SessionAction::Hibernate, &config).is_none());
        assert!(confirmation_spec(SessionAction::Reboot, &config).is_none());
        assert!(confirmation_spec(SessionAction::PowerOff, &config).is_none());
    }

    #[test]
    fn every_confirmed_action_has_specific_button_label() {
        let config = Config::default();

        let actions = [
            (SessionAction::Logout, "Log Out"),
            (SessionAction::Suspend, "Suspend"),
            (SessionAction::Hibernate, "Hibernate"),
            (SessionAction::Reboot, "Restart"),
            (SessionAction::PowerOff, "Shut Down"),
        ];

        for (action, label) in actions {
            let spec = confirmation_spec(action, &config).unwrap();
            assert_eq!(spec.accept_label, label);
        }
    }
}
