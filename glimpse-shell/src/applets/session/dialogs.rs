use std::time::Duration;

use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, gdk, glib, prelude::*};

use crate::services::session::SessionAction;

use super::Config;

const DIALOG_WIDTH: i32 = 420;
const DIALOG_HORIZONTAL_MARGIN: i32 = 48;
const ACCEPT_BUTTON_CLASS: &str = "suggested-action";
const OVERLAY_NAMESPACE: &str = "glimpse-session-confirmation";
const OVERLAY_CLOSE_ANIMATION: Duration = Duration::from_millis(75);

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
    let window = gtk::Window::new();
    let parent_window = parent.root().and_downcast::<gtk::Window>();
    if let Some(application) = parent_window
        .as_ref()
        .and_then(|window| window.application())
    {
        window.set_application(Some(&application));
    }
    let monitor = parent_monitor(parent_window.as_ref()).or_else(first_monitor);
    init_overlay_window(&window, monitor.as_ref());

    let backdrop = gtk::Overlay::new();
    backdrop.add_css_class("session-confirmation-overlay");
    backdrop.set_halign(gtk::Align::Fill);
    backdrop.set_valign(gtk::Align::Fill);
    backdrop.set_hexpand(true);
    backdrop.set_vexpand(true);
    let backdrop_fill = gtk::Box::new(gtk::Orientation::Vertical, 0);
    backdrop_fill.set_halign(gtk::Align::Fill);
    backdrop_fill.set_valign(gtk::Align::Fill);
    backdrop_fill.set_hexpand(true);
    backdrop_fill.set_vexpand(true);
    backdrop.set_child(Some(&backdrop_fill));

    let dialog = gtk::Box::new(gtk::Orientation::Vertical, 0);
    dialog.add_css_class("session-confirmation-dialog");
    dialog.set_width_request(dialog_width(monitor.as_ref()));
    dialog.set_halign(gtk::Align::Center);
    dialog.set_valign(gtk::Align::Center);

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
        let window = window.clone();
        cancel_button.connect_clicked(move |_| {
            close_with_animation(&window);
        });
    }

    let accept_button = gtk::Button::with_label(spec.accept_label);
    accept_button.add_css_class("session-confirmation-dialog__button");
    accept_button.add_css_class(ACCEPT_BUTTON_CLASS);
    {
        let window = window.clone();
        accept_button.connect_clicked(move |_| {
            on_accept();
            close_with_animation(&window);
        });
    }

    actions.append(&cancel_button);
    actions.append(&accept_button);

    content.append(&message);
    content.append(&actions);

    dialog.append(&content);
    backdrop.add_overlay(&dialog);
    window.set_child(Some(&backdrop));
    install_cancel_shortcut(&window);
    install_primary_focus(&window, &accept_button);
    present_with_animation(&window, &accept_button);
}

fn parent_monitor(parent_window: Option<&gtk::Window>) -> Option<gdk::Monitor> {
    let surface = parent_window?.surface()?;
    surface.display().monitor_at_surface(&surface)
}

fn first_monitor() -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    display.monitors().item(0).and_downcast::<gdk::Monitor>()
}

fn dialog_width(monitor: Option<&gdk::Monitor>) -> i32 {
    clamp_dialog_width(monitor.map(|monitor| monitor.geometry().width()))
}

fn clamp_dialog_width(monitor_width: Option<i32>) -> i32 {
    let Some(monitor_width) = monitor_width else {
        return DIALOG_WIDTH;
    };

    let available_width = monitor_width - DIALOG_HORIZONTAL_MARGIN;
    DIALOG_WIDTH.min(available_width.max(1))
}

fn init_overlay_window(window: &gtk::Window, monitor: Option<&gdk::Monitor>) {
    window.add_css_class("session-confirmation-overlay-window");
    window.set_decorated(false);
    window.set_deletable(false);
    window.set_resizable(false);
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(OVERLAY_NAMESPACE);
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_margin(Edge::Top, 0);
    window.set_margin(Edge::Right, 0);
    window.set_margin(Edge::Bottom, 0);
    window.set_margin(Edge::Left, 0);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);

    if let Some(monitor) = monitor {
        window.set_monitor(monitor);
        let geometry = monitor.geometry();
        window.set_default_size(geometry.width(), geometry.height());
        window.set_size_request(geometry.width(), geometry.height());
    }
}

fn install_cancel_shortcut(window: &gtk::Window) {
    let controller = gtk::EventControllerKey::new();
    let close_window = window.clone();
    controller.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::Escape {
            close_with_animation(&close_window);
            return glib::Propagation::Stop;
        }

        glib::Propagation::Proceed
    });
    window.add_controller(controller);
}

fn install_primary_focus(window: &gtk::Window, primary_button: &gtk::Button) {
    primary_button.set_focusable(true);
    primary_button.set_receives_default(true);
    gtk::prelude::GtkWindowExt::set_default_widget(window, Some(primary_button));

    let weak_window = window.downgrade();
    let weak_primary_button = primary_button.downgrade();
    window.connect_map(move |_| {
        if let (Some(window), Some(primary_button)) =
            (weak_window.upgrade(), weak_primary_button.upgrade())
        {
            focus_primary_button(&window, &primary_button);
        }
    });

    let weak_window = window.downgrade();
    let weak_primary_button = primary_button.downgrade();
    window.connect_is_active_notify(move |_| {
        if let (Some(window), Some(primary_button)) =
            (weak_window.upgrade(), weak_primary_button.upgrade())
        {
            if !window.is_active() {
                return;
            }

            focus_primary_button(&window, &primary_button);
        }
    });
}

fn present_with_animation(window: &gtk::Window, primary_button: &gtk::Button) {
    window.present();

    let window = window.clone();
    let primary_button = primary_button.clone();
    glib::idle_add_local_once(move || {
        focus_primary_button(&window, &primary_button);
        window.add_css_class("is-open");
    });
}

fn focus_primary_button(window: &gtk::Window, primary_button: &gtk::Button) {
    gtk::prelude::GtkWindowExt::set_focus_visible(window, true);
    gtk::prelude::GtkWindowExt::set_focus(window, Some(primary_button));
    primary_button.grab_focus();
}

fn close_with_animation(window: &gtk::Window) {
    if window.has_css_class("is-closing") {
        return;
    }

    window.remove_css_class("is-open");
    window.add_css_class("is-closing");

    let window = window.clone();
    glib::timeout_add_local_once(OVERLAY_CLOSE_ANIMATION, move || {
        window.close();
    });
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
    fn dialog_width_clamps_to_available_monitor_width() {
        assert_eq!(clamp_dialog_width(None), DIALOG_WIDTH);
        assert_eq!(clamp_dialog_width(Some(1920)), DIALOG_WIDTH);
        assert_eq!(
            clamp_dialog_width(Some(DIALOG_WIDTH)),
            DIALOG_WIDTH - DIALOG_HORIZONTAL_MARGIN
        );
        assert_eq!(clamp_dialog_width(Some(1)), 1);
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
