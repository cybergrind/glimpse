use glimpse::providers::power::{PowerProfiles, PowerProvider};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct PowerProfileList {
    conn: zbus::Connection,
    list: gtk::Box,
}

pub struct PowerProfileListInit {
    pub conn: zbus::Connection,
}

#[derive(Debug)]
pub enum PowerProfileListInput {
    Update(PowerProfiles),
    SetProfile(String),
}

impl SimpleComponent for PowerProfileList {
    type Init = PowerProfileListInit;
    type Input = PowerProfileListInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.add_css_class("power-profile-section");

        let label = gtk::Label::new(Some("Power profile"));
        label.set_halign(gtk::Align::Start);
        label.add_css_class("detail-key");
        root.append(&label);

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.add_css_class("profile-list");
        root.append(&list);

        let model = PowerProfileList {
            conn: init.conn,
            list,
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            PowerProfileListInput::SetProfile(profile) => {
                let conn = self.conn.clone();
                gtk::glib::spawn_future_local(async move {
                    if let Err(e) = PowerProvider::new(conn).set_profile(&profile).await {
                        tracing::warn!("set profile failed: {e}");
                    }
                });
            }
            PowerProfileListInput::Update(profiles) => {
                while let Some(child) = self.list.first_child() {
                    self.list.remove(&child);
                }

                for name in &profiles.available {
                    if name.is_empty() {
                        continue;
                    }

                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    row.add_css_class("profile-row");

                    let icon = gtk::Image::from_icon_name(profile_icon(name));
                    icon.set_pixel_size(16);
                    icon.add_css_class("profile-icon");
                    row.append(&icon);

                    let label = gtk::Label::new(Some(profile_display_name(name)));
                    label.set_hexpand(true);
                    label.set_halign(gtk::Align::Start);
                    row.append(&label);

                    if *name == profiles.active {
                        let check = gtk::Image::from_icon_name("object-select-symbolic");
                        check.set_pixel_size(14);
                        check.add_css_class("profile-check");
                        row.append(&check);
                    }

                    let btn = gtk::Button::new();
                    btn.set_child(Some(&row));
                    btn.add_css_class("flat");
                    btn.add_css_class("profile-btn");

                    let sender = sender.clone();
                    let profile = name.clone();
                    btn.connect_clicked(move |_| {
                        sender.input(PowerProfileListInput::SetProfile(profile.clone()));
                    });

                    self.list.append(&btn);
                }

            }
        }
    }
}

fn profile_icon(profile: &str) -> &'static str {
    match profile {
        "power-saver" => "power-profile-power-saver-symbolic",
        "balanced" => "power-profile-balanced-symbolic",
        "performance" => "power-profile-performance-symbolic",
        _ => "power-profile-balanced-symbolic",
    }
}

fn profile_display_name(profile: &str) -> &'static str {
    match profile {
        "power-saver" => "Power Saver",
        "balanced" => "Balanced",
        "performance" => "Performance",
        _ => "Unknown",
    }
}
