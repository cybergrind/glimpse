use std::collections::HashMap;

use glimpse::power::provider::PowerProfiles;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

struct ProfileRow {
    button: gtk::Button,
    check: gtk::Image,
}

pub struct PowerProfileList {
    list: gtk::Box,
    rows: HashMap<String, ProfileRow>,
    order: Vec<String>,
    active: String,
}

#[derive(Debug)]
pub enum PowerProfileListInput {
    Update(PowerProfiles),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowerProfileListOutput {
    SetProfile(String),
}

impl SimpleComponent for PowerProfileList {
    type Init = ();
    type Input = PowerProfileListInput;
    type Output = PowerProfileListOutput;
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        _init: Self::Init,
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
            list,
            rows: HashMap::new(),
            order: Vec::new(),
            active: String::new(),
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            PowerProfileListInput::Update(profiles) => {
                let desired_order: Vec<String> = profiles
                    .available
                    .iter()
                    .filter(|name| !name.is_empty())
                    .cloned()
                    .collect();

                if self.order != desired_order {
                    let removed: Vec<String> = self
                        .rows
                        .keys()
                        .filter(|name| !desired_order.contains(*name))
                        .cloned()
                        .collect();

                    for name in removed {
                        if let Some(row) = self.rows.remove(&name) {
                            self.list.remove(&row.button);
                        }
                    }

                    for name in &desired_order {
                        if !self.rows.contains_key(name) {
                            self.rows.insert(name.clone(), build_profile_row(name, sender.clone()));
                        }
                    }

                    while let Some(child) = self.list.first_child() {
                        self.list.remove(&child);
                    }

                    for name in &desired_order {
                        if let Some(row) = self.rows.get(name) {
                            self.list.append(&row.button);
                        }
                    }

                    self.order = desired_order;
                }

                if self.active != profiles.active {
                    if let Some(row) = self.rows.get(&self.active) {
                        row.check.set_visible(false);
                    }
                    if let Some(row) = self.rows.get(&profiles.active) {
                        row.check.set_visible(true);
                    }
                    self.active = profiles.active.clone();
                }

                for (name, row) in &self.rows {
                    row.check.set_visible(*name == profiles.active);
                }
            }
        }
    }
}

fn build_profile_row(name: &str, sender: ComponentSender<PowerProfileList>) -> ProfileRow {
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

    let check = gtk::Image::from_icon_name("object-select-symbolic");
    check.set_pixel_size(14);
    check.add_css_class("profile-check");
    check.set_visible(false);
    row.append(&check);

    let button = gtk::Button::new();
    button.set_child(Some(&row));
    button.add_css_class("flat");
    button.add_css_class("profile-btn");

    let profile = name.to_string();
    button.connect_clicked(move |_| {
        let _ = sender.output(PowerProfileListOutput::SetProfile(profile.clone()));
    });

    ProfileRow { button, check }
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
