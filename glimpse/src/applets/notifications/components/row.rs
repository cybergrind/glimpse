use std::cell::RefCell;
use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gdk, gio, glib, prelude::*},
};

use super::{NotifData, NotificationCommandEmitter};
use crate::applets::notifications::NotificationActionCommand;
use crate::applets::notifications::activation::{default_action_command, invoke_action_command};

pub struct NotificationCard {
    root: gtk::Box,
    icon: gtk::Image,
    app_label: gtk::Label,
    time_label: gtk::Label,
    dismiss_btn: gtk::Button,
    image: gtk::Picture,
    content: gtk::Box,
    copy: gtk::Box,
    summary_label: gtk::Label,
    body_label: gtk::Label,
    actions_box: gtk::Box,
    emit_command: NotificationCommandEmitter,
    current: Rc<RefCell<NotifData>>,
    notif: NotifData,
    role: NotificationCardRole,
}

pub struct NotificationCardInit {
    pub notif: NotifData,
    pub emit_command: NotificationCommandEmitter,
    pub role: NotificationCardRole,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationCardRole {
    Full,
    SecondInStack,
    LowerInStack,
}

#[derive(Debug)]
pub enum NotificationCardInput {
    Update(NotifData),
    SetRole(NotificationCardRole),
    ActivateDefault(u32, Option<String>, String, u32),
    Dismiss(u32),
    InvokeAction(u32, String),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for NotificationCard {
    type Init = NotificationCardInit;
    type Input = NotificationCardInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 4,
            add_css_class: "notif-card",

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender, current = current.clone()] => move |gesture, _, _, _| {
                    let notif = current.borrow().clone();
                    if !notif.actions.iter().any(|(key, _)| key == "default") {
                        return;
                    }
                    let timestamp = gesture.current_event_time();
                    sender.input(NotificationCardInput::ActivateDefault(
                        notif.id,
                        notif.desktop_entry.clone(),
                        notif.app_name.clone(),
                        timestamp,
                    ));
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            },

            add_controller = gtk::GestureClick {
                set_button: 3,
                connect_pressed[sender, current = current.clone()] => move |gesture, _, _, _| {
                    sender.input(NotificationCardInput::Dismiss(current.borrow().id));
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            },

            #[name(header)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                #[name(icon)]
                gtk::Image {
                    add_css_class: "notif-card-icon",
                    add_css_class: "notif-icon",
                    set_valign: gtk::Align::Center,
                },

                #[name(app_label)]
                gtk::Label {
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                    add_css_class: "notif-app-name",
                },

                #[name(time_label)]
                gtk::Label {
                    set_xalign: 0.0,
                    add_css_class: "notif-time",
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_hexpand: true,
                },

                #[name(dismiss_btn)]
                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "notif-dismiss",
                    set_icon_name: "window-close-symbolic",
                    set_valign: gtk::Align::Center,
                    connect_clicked[sender, current = current.clone()] => move |_| {
                        sender.input(NotificationCardInput::Dismiss(current.borrow().id));
                    },
                },
            },

            #[name(content)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                set_hexpand: true,
                add_css_class: "notif-content",

                #[name(image)]
                gtk::Picture {
                    set_can_shrink: true,
                    set_content_fit: gtk::ContentFit::Contain,
                    set_valign: gtk::Align::Start,
                    add_css_class: "notification-inline-image",
                    add_css_class: "notif-inline-image",
                    set_visible: false,
                },

                #[name(copy)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 4,
                    set_hexpand: true,
                    add_css_class: "notif-copy",

                    #[name(summary_label)]
                    gtk::Label {
                        set_halign: gtk::Align::Start,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_max_width_chars: 40,
                        add_css_class: "notif-summary",
                    },

                    #[name(body_label)]
                    gtk::Label {
                        set_halign: gtk::Align::Start,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_max_width_chars: 45,
                        set_lines: 2,
                        set_wrap: true,
                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        add_css_class: "notif-body",
                    },

                    #[name(actions_box)]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 4,
                        add_css_class: "notif-actions",
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let current = Rc::new(RefCell::new(init.notif.clone()));

        let widgets = view_output!();

        let mut model = NotificationCard {
            root: root.clone(),
            icon: widgets.icon.clone(),
            app_label: widgets.app_label.clone(),
            time_label: widgets.time_label.clone(),
            dismiss_btn: widgets.dismiss_btn.clone(),
            image: widgets.image.clone(),
            content: widgets.content.clone(),
            copy: widgets.copy.clone(),
            summary_label: widgets.summary_label.clone(),
            body_label: widgets.body_label.clone(),
            actions_box: widgets.actions_box.clone(),
            emit_command: init.emit_command,
            current: current.clone(),
            notif: init.notif,
            role: init.role,
        };
        model.refresh(&sender);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NotificationCardInput::Update(notif) => {
                *self.current.borrow_mut() = notif.clone();
                self.notif = notif;
            }
            NotificationCardInput::SetRole(role) => {
                self.role = role;
            }
            NotificationCardInput::ActivateDefault(id, desktop_entry, app_name, timestamp) => {
                let emit_command = self.emit_command.clone();
                glib::spawn_future_local(async move {
                    emit_command(
                        default_action_command(id, desktop_entry, app_name, timestamp).await,
                    );
                });
            }
            NotificationCardInput::Dismiss(id) => {
                (self.emit_command)(NotificationActionCommand::Dismiss { id });
            }
            NotificationCardInput::InvokeAction(id, action_key) => {
                (self.emit_command)(invoke_action_command(id, &action_key, None));
            }
        }

        self.refresh(&sender);
    }
}

impl NotificationCard {
    fn refresh(&mut self, sender: &ComponentSender<Self>) {
        self.root.remove_css_class("notif-group-second");
        self.root.remove_css_class("notif-group-lower");
        match self.role {
            NotificationCardRole::Full => {}
            NotificationCardRole::SecondInStack => {
                self.root.add_css_class("notif-group-second");
            }
            NotificationCardRole::LowerInStack => {
                self.root.add_css_class("notif-group-lower");
            }
        }

        self.root.set_tooltip_text(Some(&self.notif.summary));
        self.icon
            .set_icon_name(Some(&resolve_notif_icon_name(&self.notif)));
        self.app_label.set_label(if self.notif.app_name.is_empty() {
            "Notification"
        } else {
            &self.notif.app_name
        });
        self.time_label.set_label(&time_ago(self.notif.timestamp));
        self.summary_label.set_label(&self.notif.summary);
        self.body_label.set_label(&self.notif.body);
        self.body_label.set_visible(!self.notif.body.is_empty());

        let interactive = self.role == NotificationCardRole::Full;
        let has_default = interactive && self.notif.actions.iter().any(|(key, _)| key == "default");
        self.root
            .set_cursor_from_name(if has_default { Some("pointer") } else { None });
        self.root.set_can_target(interactive);
        self.dismiss_btn.set_visible(interactive);

        let mut child = self.actions_box.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            self.actions_box.remove(&widget);
        }

        let visible_actions: Vec<&(String, String)> = self
            .notif
            .actions
            .iter()
            .filter(|(key, _)| key != "default")
            .collect();
        self.actions_box
            .set_visible(interactive && !visible_actions.is_empty());
        if interactive {
            for (key, label) in visible_actions {
                let action_btn = gtk::Button::with_label(label);
                action_btn.add_css_class("flat");
                action_btn.add_css_class("notif-action-btn");
                let action_key = key.clone();
                let id = self.notif.id;
                action_btn.connect_clicked({
                    let sender = sender.clone();
                    move |_| {
                        sender.input(NotificationCardInput::InvokeAction(id, action_key.clone()))
                    }
                });
                self.actions_box.append(&action_btn);
            }
        }

        if let Some(texture) = load_notification_image_texture(&self.notif) {
            self.image.set_paintable(Some(&texture));
            self.image.set_visible(interactive);
        } else {
            self.image.set_paintable(None::<&gdk::Paintable>);
            self.image.set_visible(false);
        }

        if self.content.last_child().as_ref() != Some(self.copy.upcast_ref::<gtk::Widget>()) {
            self.content.append(&self.copy);
        }
    }
}

fn base_notif_icon_name(notif: &NotifData) -> Option<&str> {
    if !notif.app_icon.is_empty() {
        return Some(&notif.app_icon);
    }
    if let Some(ref de) = notif.desktop_entry {
        if !de.is_empty() {
            return Some(de);
        }
    }
    None
}

fn icon_exists(name: &str) -> bool {
    gdk::Display::default()
        .map(|display| gtk::IconTheme::for_display(&display).has_icon(name))
        .unwrap_or(false)
}

pub fn resolve_notif_icon_name(notif: &NotifData) -> String {
    let Some(base_name) = base_notif_icon_name(notif) else {
        return "dialog-information-symbolic".into();
    };

    if base_name.ends_with("-symbolic") && icon_exists(base_name) {
        return base_name.to_string();
    }

    let symbolic_name = format!("{base_name}-symbolic");
    if icon_exists(&symbolic_name) {
        return symbolic_name;
    }

    if icon_exists(base_name) {
        return base_name.to_string();
    }

    "dialog-information-symbolic".into()
}

pub fn build_notification_icon(notif: &NotifData, css_class: &str) -> gtk::Image {
    let icon = gtk::Image::from_icon_name(&resolve_notif_icon_name(notif));
    icon.add_css_class(css_class);
    icon
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NotificationImageSource {
    FilePath(String),
    FileUri(String),
}

fn parse_notification_image_source(value: Option<&str>) -> Option<NotificationImageSource> {
    let value = value.map(str::trim).filter(|value| !value.is_empty())?;
    if value.starts_with("file://") {
        Some(NotificationImageSource::FileUri(value.to_string()))
    } else if value.starts_with('/') {
        Some(NotificationImageSource::FilePath(value.to_string()))
    } else {
        None
    }
}

pub fn load_notification_image_texture(notif: &NotifData) -> Option<gdk::Texture> {
    let source = parse_notification_image_source(notif.image.as_deref())?;
    let file = match source {
        NotificationImageSource::FilePath(path) => gio::File::for_path(path),
        NotificationImageSource::FileUri(uri) => gio::File::for_uri(&uri),
    };
    gdk::Texture::from_file(&file).ok()
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
        assert_eq!(format_time_diff(259200), "3d");
    }

    #[test]
    fn notification_image_source_uses_file_path_for_absolute_paths() {
        assert_eq!(
            parse_notification_image_source(Some("/tmp/demo.png")),
            Some(NotificationImageSource::FilePath("/tmp/demo.png".into()))
        );
    }

    #[test]
    fn notification_image_source_uses_file_uri_for_file_uris() {
        assert_eq!(
            parse_notification_image_source(Some("file:///tmp/demo.png")),
            Some(NotificationImageSource::FileUri(
                "file:///tmp/demo.png".into()
            ))
        );
    }

    #[test]
    fn notification_image_source_ignores_empty_or_unknown_values() {
        assert_eq!(parse_notification_image_source(None), None);
        assert_eq!(parse_notification_image_source(Some("")), None);
        assert_eq!(
            parse_notification_image_source(Some("https://example.com/demo.png")),
            None
        );
    }
}
