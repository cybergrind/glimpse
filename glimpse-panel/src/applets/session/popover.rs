use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use super::config::SessionConfig;

pub struct SessionPopover {
    popover: gtk::Popover,
}

pub struct SessionPopoverInit {
    pub parent: gtk::Box,
    pub client: Arc<Client>,
    pub config: SessionConfig,
}

#[derive(Debug)]
pub enum SessionPopoverInput {
    Toggle,
    UpdateActions(serde_json::Value),
}

fn spawn_call(client: &Arc<Client>, method: &'static str) {
    let c = client.clone();
    glib::spawn_future_local(async move {
        let _ = c.call(method, serde_json::json!({})).await;
    });
}

fn confirm_and_call(
    client: &Arc<Client>,
    method: &'static str,
    title: &str,
    message: &str,
    popover: &gtk::Popover,
) {
    let dialog = gtk::MessageDialog::new(
        gtk::Window::NONE,
        gtk::DialogFlags::MODAL,
        gtk::MessageType::Question,
        gtk::ButtonsType::None,
        message,
    );
    dialog.set_title(Some(title));
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button(title, gtk::ResponseType::Accept);

    let c = client.clone();
    let pop = popover.clone();
    dialog.connect_response(move |dlg, response| {
        dlg.close();
        if response == gtk::ResponseType::Accept {
            pop.popdown();
            spawn_call(&c, method);
        }
    });
    dialog.present();
}

impl SimpleComponent for SessionPopover {
    type Init = SessionPopoverInit;
    type Input = SessionPopoverInput;
    type Output = ();
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Popover::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("session-popover");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        // === Hero ===
        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hero.add_css_class("session-hero");

        let avatar = gtk::Image::from_icon_name("avatar-default-symbolic");
        avatar.set_pixel_size(32);
        hero.append(&avatar);

        let info_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        info_box.set_hexpand(true);
        info_box.set_valign(gtk::Align::Center);

        let username = std::env::var("USER").unwrap_or_else(|_| "user".into());
        let user_label = gtk::Label::new(Some(&username));
        user_label.set_halign(gtk::Align::Start);
        user_label.add_css_class("session-hero-name");
        info_box.append(&user_label);

        let subtitle = format_subtitle();
        let subtitle_label = gtk::Label::new(Some(&subtitle));
        subtitle_label.set_halign(gtk::Align::Start);
        subtitle_label.add_css_class("session-hero-subtitle");
        info_box.append(&subtitle_label);

        hero.append(&info_box);
        vbox.append(&hero);

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let config = &init.config;
        let client = &init.client;
        let popover = &root;

        // === Session actions ===
        if config.show_lock {
            let c = client.clone();
            vbox.append(&build_action_row(
                "system-lock-screen-symbolic",
                "Lock Screen",
                move |_| {
                    spawn_call(&c, "power.lock");
                },
            ));
        }

        if config.show_logout {
            let c = client.clone();
            let confirm = config.confirm_logout;
            let p = popover.clone();
            vbox.append(&build_action_row(
                "system-log-out-symbolic",
                "Log Out",
                move |_| {
                    if confirm {
                        confirm_and_call(
                            &c,
                            "power.lock",
                            "Log Out",
                            "Are you sure you want to log out?",
                            &p,
                        );
                    } else {
                        spawn_call(&c, "power.lock");
                    }
                },
            ));
        }

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Power actions ===
        if config.show_suspend {
            let c = client.clone();
            vbox.append(&build_action_row(
                "media-playback-pause-symbolic",
                "Suspend",
                move |_| {
                    spawn_call(&c, "power.suspend");
                },
            ));
        }

        if config.show_hibernate {
            let c = client.clone();
            vbox.append(&build_action_row(
                "document-save-symbolic",
                "Hibernate",
                move |_| {
                    spawn_call(&c, "power.hibernate");
                },
            ));
        }

        if config.show_reboot {
            let c = client.clone();
            let confirm = config.confirm_reboot;
            let p = popover.clone();
            vbox.append(&build_action_row(
                "system-reboot-symbolic",
                "Restart",
                move |_| {
                    if confirm {
                        confirm_and_call(
                            &c,
                            "power.reboot",
                            "Restart",
                            "Are you sure you want to restart?",
                            &p,
                        );
                    } else {
                        spawn_call(&c, "power.reboot");
                    }
                },
            ));
        }

        if config.show_shutdown {
            let c = client.clone();
            let confirm = config.confirm_shutdown;
            let p = popover.clone();
            vbox.append(&build_action_row(
                "system-shutdown-symbolic",
                "Shut Down",
                move |_| {
                    if confirm {
                        confirm_and_call(
                            &c,
                            "power.poweroff",
                            "Shut Down",
                            "Are you sure you want to shut down?",
                            &p,
                        );
                    } else {
                        spawn_call(&c, "power.poweroff");
                    }
                },
            ));
        }

        root.set_child(Some(&vbox));

        let model = SessionPopover {
            popover: root.clone(),
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            SessionPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            SessionPopoverInput::UpdateActions(_) => {}
        }
    }
}

fn build_action_row<F: Fn(&gtk::Button) + 'static>(
    icon_name: &str,
    label: &str,
    on_click: F,
) -> gtk::Button {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("session-action-row");

    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(16);
    icon.add_css_class("session-action-icon");
    row.append(&icon);

    let lbl = gtk::Label::new(Some(label));
    lbl.set_hexpand(true);
    lbl.set_halign(gtk::Align::Start);
    row.append(&lbl);

    let btn = gtk::Button::new();
    btn.set_child(Some(&row));
    btn.add_css_class("flat");
    btn.add_css_class("session-action-btn");
    btn.connect_clicked(on_click);
    btn
}

fn format_subtitle() -> String {
    let hostname = gethostname();
    let uptime = read_uptime();
    if uptime.is_empty() {
        hostname
    } else {
        format!("{hostname} · up {uptime}")
    }
}

fn gethostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|_| "linux".into())
}

fn read_uptime() -> String {
    let secs: f64 = std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next()?.parse().ok())
        .unwrap_or(0.0);
    let secs = secs as u64;
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}
