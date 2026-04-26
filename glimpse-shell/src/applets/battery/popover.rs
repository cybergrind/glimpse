#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::{
        key_value_grid::{KeyValueGrid, KeyValueGridInit, KeyValueGridInput, KeyValueItem},
        popover_shell::PopoverShell,
    },
    services::{battery::BatteryStatus, power::PowerProfiles},
};

use super::components::degraded::{DegradedWarning, DegradedWarningInput};
use super::components::hero::{BatteryHero, BatteryHeroInput};
use super::components::profiles::{
    PowerProfileList, PowerProfileListInput, PowerProfileListOutput,
};
pub struct Popover {
    popover: gtk::Popover,
    hero: Controller<BatteryHero>,
    details: Controller<KeyValueGrid>,
    profiles: Controller<PowerProfileList>,
    degraded: Controller<DegradedWarning>,
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
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[local_ref]
                    hero_widget -> gtk::Box {},

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

                    #[local_ref]
                    degraded_widget -> gtk::Box {},
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let hero = BatteryHero::builder().launch(()).detach();
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
        let profiles =
            PowerProfileList::builder()
                .launch(())
                .forward(sender.output_sender(), |output| match output {
                    PowerProfileListOutput::SetProfile(profile) => {
                        PopoverOutput::SetProfile(profile)
                    }
                });
        let degraded = DegradedWarning::builder().launch(()).detach();

        let hero_widget = hero.widget().clone();
        let details_widget = details.widget().clone();
        let profiles_widget = profiles.widget().clone();
        let degraded_widget = degraded.widget().clone();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let model = Popover {
            popover: widgets.root.clone(),
            hero,
            details,
            profiles,
            degraded,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            PopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            PopoverInput::UpdateStatus(status) => {
                self.hero.emit(BatteryHeroInput::Update(status.clone()));
                self.details.emit(KeyValueGridInput::Update(vec![
                    KeyValueItem {
                        label: "Health".into(),
                        value: format!("{:.0}%", status.capacity),
                        visible: true,
                    },
                    KeyValueItem {
                        label: "Model".into(),
                        value: if status.model.is_empty() {
                            "\u{2014}".into()
                        } else {
                            status.model
                        },
                        visible: true,
                    },
                    KeyValueItem {
                        label: "Charge limit".into(),
                        value: format!("{}%", status.charge_threshold),
                        visible: status.charge_threshold > 0,
                    },
                    KeyValueItem {
                        label: "Rate".into(),
                        value: format!("{:.1}W", status.energy_rate),
                        visible: status.energy_rate > 0.0,
                    },
                ]));
            }
            PopoverInput::UpdateProfiles(profiles) => {
                self.degraded.emit(DegradedWarningInput::Update(
                    profiles.performance_degraded.clone(),
                ));
                self.profiles.emit(PowerProfileListInput::Update(profiles));
            }
        }
    }
}
