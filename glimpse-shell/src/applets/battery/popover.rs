#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::{
        action_menu::{
            ActionMenu, ActionMenuItem, Init as ActionMenuInit, Input as ActionMenuInput,
        },
        animated_popover::AnimatedPopover,
        key_value_grid::{KeyValueGrid, KeyValueGridInit, KeyValueGridInput, KeyValueItem},
        popover_shell::PopoverShell,
    },
    services::{battery::BatteryStatus, power::PowerProfiles},
};

use super::components::degraded::DegradedWarningView;
use super::components::hero::BatteryHeroView;
use super::format;
pub struct Popover {
    animation: AnimatedPopover,
    hero_icon_name: String,
    hero_percentage: String,
    hero_progress: f64,
    hero_state: String,
    details: Controller<KeyValueGrid>,
    profiles: Controller<ActionMenu<String>>,
    degraded_visible: bool,
    degraded_text: String,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateStatus(BatteryStatus),
    UpdateProfiles(PowerProfiles),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    SetProfile(String),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "battery-popover",
            add_css_class: "popover-size-small",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[name = "hero"]
                    #[template]
                    BatteryHeroView,

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[local_ref]
                    details_widget -> gtk::Box {},

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[local_ref]
                    profiles_widget -> gtk::Box {},

                    #[name = "degraded"]
                    #[template]
                    DegradedWarningView,
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let details = KeyValueGrid::builder()
            .launch(KeyValueGridInit {
                values: vec![
                    KeyValueItem {
                        label: "Health".into(),
                        value: "".into(),
                        visible: true,
                    },
                    KeyValueItem {
                        label: "Model".into(),
                        value: "".into(),
                        visible: true,
                    },
                    KeyValueItem {
                        label: "Charge limit".into(),
                        value: "".into(),
                        visible: false,
                    },
                    KeyValueItem {
                        label: "Rate".into(),
                        value: "".into(),
                        visible: false,
                    },
                ],
            })
            .detach();
        let profiles = ActionMenu::builder()
            .launch(ActionMenuInit {
                header: Some("Power profile".into()),
                items: Vec::new(),
            })
            .forward(sender.output_sender(), PopoverOutput::SetProfile);
        let details_widget = details.widget().clone();
        let profiles_widget = profiles.widget().clone();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            hero_icon_name: "battery-missing-symbolic".into(),
            hero_percentage: "\u{2014}".into(),
            hero_progress: 0.0,
            hero_state: String::new(),
            details,
            profiles,
            degraded_visible: false,
            degraded_text: String::new(),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            PopoverInput::Toggle => {
                self.animation.toggle();
            }
            PopoverInput::UpdateStatus(status) => {
                self.hero_icon_name = status.icon_name.clone();
                self.hero_percentage = format::percent(status.percentage);
                self.hero_progress = status.percentage as f64 / 100.0;
                self.hero_state = format::state_text(&status);
                self.details.emit(KeyValueGridInput::Update(vec![
                    KeyValueItem {
                        label: "Health".into(),
                        value: format::percent(status.capacity),
                        visible: true,
                    },
                    KeyValueItem {
                        label: "Model".into(),
                        value: format::optional_model(status.model),
                        visible: true,
                    },
                    KeyValueItem {
                        label: "Charge limit".into(),
                        value: format::percent(status.charge_threshold),
                        visible: status.charge_threshold > 0,
                    },
                    KeyValueItem {
                        label: "Rate".into(),
                        value: format::power_rate(status.energy_rate),
                        visible: status.energy_rate > 0.0,
                    },
                ]));
            }
            PopoverInput::UpdateProfiles(profiles) => {
                self.degraded_visible = !profiles.performance_degraded.is_empty();
                self.degraded_text = format::degraded_warning(&profiles.performance_degraded);
                self.profiles
                    .emit(ActionMenuInput::Update(build_profile_items(&profiles)));
            }
        }
    }

    fn post_view() {
        hero.icon.set_icon_name(Some(&model.hero_icon_name));
        hero.percentage.set_label(&model.hero_percentage);
        hero.progress.set_fraction(model.hero_progress);
        hero.state.set_label(&model.hero_state);

        degraded.as_ref().set_visible(model.degraded_visible);
        degraded.label.set_label(&model.degraded_text);
    }
}

fn build_profile_items(profiles: &PowerProfiles) -> Vec<ActionMenuItem<String>> {
    profiles
        .available
        .iter()
        .filter(|profile| !profile.is_empty())
        .map(|profile| ActionMenuItem {
            label: format::profile_label(profile).into(),
            icon: Some(format::profile_icon(profile).into()),
            visible: true,
            checked: Some(profile == &profiles.active),
            selectable: Some(true),
            command: profile.clone(),
        })
        .collect()
}
