use glimpse::power::provider::PowerProfiles;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct PowerProfileRowView {
    profile: String,
    display: &'static str,
    active: bool,
}

struct PowerProfileRow {
    profile: String,
    display: &'static str,
    active: bool,
}

#[derive(Debug)]
enum PowerProfileRowInput {
    SetActive(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PowerProfileRowOutput {
    SetProfile(String),
}

#[relm4::component]
impl SimpleComponent for PowerProfileRow {
    type Init = PowerProfileRowView;
    type Input = PowerProfileRowInput;
    type Output = PowerProfileRowOutput;

    view! {
        root = gtk::Button {
            add_css_class: "flat",
            add_css_class: "action-row",
            add_css_class: "action-row__button",
            add_css_class: "profile-btn",
            connect_clicked[sender, profile = model.profile.clone()] => move |_| {
                let _ = sender.output(PowerProfileRowOutput::SetProfile(profile.clone()));
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "profile-row",
                add_css_class: "action-row__content-shell",

                gtk::Image {
                    set_icon_name: Some(profile_icon(&model.profile)),
                    set_pixel_size: 16,
                    add_css_class: "profile-icon",
                    add_css_class: "action-row__leading",
                },

                gtk::Label {
                    set_label: model.display,
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    add_css_class: "action-row__title",
                },

                gtk::Image {
                    set_icon_name: Some("object-select-symbolic"),
                    set_pixel_size: 14,
                    add_css_class: "profile-check",
                    add_css_class: "action-row__trailing",
                    #[watch]
                    set_visible: model.active,
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = PowerProfileRow {
            profile: init.profile,
            display: init.display,
            active: init.active,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let PowerProfileRowInput::SetActive(active) = msg;
        self.active = active;
    }
}

pub struct PowerProfileList {
    rows: Vec<Controller<PowerProfileRow>>,
    list: gtk::Box,
}

#[derive(Debug)]
pub enum PowerProfileListInput {
    Update(PowerProfiles),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowerProfileListOutput {
    SetProfile(String),
}

#[relm4::component(pub)]
impl SimpleComponent for PowerProfileList {
    type Init = ();
    type Input = PowerProfileListInput;
    type Output = PowerProfileListOutput;

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "power-profile-section",
            add_css_class: "section-block",

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "section-block__header",

                gtk::Label {
                    set_label: "Power profile",
                    set_halign: gtk::Align::Start,
                    add_css_class: "section-block__title",
                },
            },

            #[name(list)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "profile-list",
                add_css_class: "section-block__body",
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = PowerProfileList {
            rows: Vec::new(),
            list: gtk::Box::new(gtk::Orientation::Vertical, 0),
        };
        let widgets = view_output!();
        let mut model = model;
        model.list = widgets.list.clone();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        let PowerProfileListInput::Update(profiles) = msg;
        let rows = build_profile_rows(&profiles);
        render_profile_rows(&self.list, &rows, &sender, &mut self.rows);
    }
}

fn build_profile_rows(profiles: &PowerProfiles) -> Vec<PowerProfileRowView> {
    profiles
        .available
        .iter()
        .filter(|name| !name.is_empty())
        .map(|name| PowerProfileRowView {
            profile: name.clone(),
            display: profile_display_name(name),
            active: name == &profiles.active,
        })
        .collect()
}

fn render_profile_rows(
    container: &gtk::Box,
    rows: &[PowerProfileRowView],
    sender: &ComponentSender<PowerProfileList>,
    row_components: &mut Vec<Controller<PowerProfileRow>>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    row_components.clear();

    for row in rows.iter().cloned() {
        let component =
            PowerProfileRow::builder()
                .launch(row)
                .forward(sender.output_sender(), |output| match output {
                    PowerProfileRowOutput::SetProfile(profile) => {
                        PowerProfileListOutput::SetProfile(profile)
                    }
                });
        container.append(component.widget());
        row_components.push(component);
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
