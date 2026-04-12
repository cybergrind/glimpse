#![allow(unused_assignments)]

use glimpse::battery::provider::BatteryStatus;
use glimpse::power::provider::PowerProfiles;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::degraded::{DegradedWarning, DegradedWarningInput};
use super::components::details::{BatteryDetails, BatteryDetailsInput};
use super::components::hero::{BatteryHero, BatteryHeroInput};
use super::components::profiles::{
    PowerProfileList, PowerProfileListInput, PowerProfileListOutput,
};

pub struct BatteryPopover {
    popover: gtk::Popover,
    hero: Controller<BatteryHero>,
    details: Controller<BatteryDetails>,
    profiles: Controller<PowerProfileList>,
    degraded: Controller<DegradedWarning>,
}

pub struct BatteryPopoverInit {
    pub parent: gtk::Box,
    pub has_settings_command: bool,
}

#[derive(Debug)]
pub enum BatteryPopoverInput {
    Toggle,
    UpdateStatus(BatteryStatus),
    UpdateProfiles(PowerProfiles),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatteryPopoverOutput {
    SetProfile(String),
    OpenSettings,
}

#[relm4::component(pub)]
impl SimpleComponent for BatteryPopover {
    type Init = BatteryPopoverInit;
    type Input = BatteryPopoverInput;
    type Output = BatteryPopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "battery-popover",

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_hexpand: false,
                set_overflow: gtk::Overflow::Hidden,

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

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_visible: init.has_settings_command,
                },

                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "settings-btn",
                    set_visible: init.has_settings_command,
                    connect_clicked[sender] => move |_| {
                        let _ = sender.output(BatteryPopoverOutput::OpenSettings);
                    },

                    gtk::Label {
                        set_label: "Power Settings",
                        set_halign: gtk::Align::Start,
                    },
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let hero = BatteryHero::builder().launch(()).detach();
        let details = BatteryDetails::builder().launch(()).detach();
        let profiles = PowerProfileList::builder()
            .launch(())
            .forward(sender.output_sender(), map_profile_output);
        let degraded = DegradedWarning::builder().launch(()).detach();

        let hero_widget = hero.widget().clone();
        let details_widget = details.widget().clone();
        let profiles_widget = profiles.widget().clone();
        let degraded_widget = degraded.widget().clone();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let model = BatteryPopover {
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
            BatteryPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            BatteryPopoverInput::UpdateStatus(status) => {
                self.hero.emit(BatteryHeroInput::Update(status.clone()));
                self.details.emit(BatteryDetailsInput::Update(status));
            }
            BatteryPopoverInput::UpdateProfiles(profiles) => {
                self.degraded.emit(DegradedWarningInput::Update(
                    profiles.performance_degraded.clone(),
                ));
                self.profiles.emit(PowerProfileListInput::Update(profiles));
            }
        }
    }
}

fn map_profile_output(output: PowerProfileListOutput) -> BatteryPopoverOutput {
    match output {
        PowerProfileListOutput::SetProfile(profile) => BatteryPopoverOutput::SetProfile(profile),
    }
}
