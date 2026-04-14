#![allow(unused_assignments)]

use glimpse::battery::provider::BatteryStatus;
use glimpse::power::provider::PowerProfiles;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::components::{
    footer_action::{FooterAction, FooterActionInit},
    popover_shell::{PopoverShell, PopoverShellInit},
};
use super::components::degraded::{DegradedWarning, DegradedWarningInput};
use super::components::details::{BatteryDetails, BatteryDetailsInput};
use super::components::hero::{BatteryHero, BatteryHeroInput};
use super::components::profiles::{
    PowerProfileList, PowerProfileListInput, PowerProfileListOutput,
};

pub struct BatteryPopover {
    popover: gtk::Popover,
    shell: Controller<PopoverShell>,
    hero: Controller<BatteryHero>,
    details: Controller<BatteryDetails>,
    profiles: Controller<PowerProfileList>,
    degraded: Controller<DegradedWarning>,
    footer: Controller<FooterAction>,
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
            set_hexpand: false,

            #[local_ref]
            shell_widget -> gtk::Box {}
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let shell = PopoverShell::builder()
            .launch(PopoverShellInit {
                show_footer: init.has_settings_command,
            })
            .detach();
        let hero = BatteryHero::builder().launch(()).detach();
        let details = BatteryDetails::builder().launch(()).detach();
        let profiles = PowerProfileList::builder()
            .launch(())
            .forward(sender.output_sender(), map_profile_output);
        let degraded = DegradedWarning::builder().launch(()).detach();
        let footer = FooterAction::builder()
            .launch(FooterActionInit {
                title: "Power Settings".into(),
                subtitle: String::new(),
            })
            .detach();

        let shell_widget = shell.widget().clone();
        let hero_widget = hero.widget().clone();
        let details_widget = details.widget().clone();
        let profiles_widget = profiles.widget().clone();
        let degraded_widget = degraded.widget().clone();
        let shell_content = shell_widget
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("popover shell should expose content box");
        let shell_footer = shell_content
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("popover shell should expose footer box");
        shell_content.append(&hero_widget);
        shell_content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        shell_content.append(&details_widget);
        shell_content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        shell_content.append(&profiles_widget);
        shell_content.append(&degraded_widget);
        shell_footer.append(footer.widget());

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let footer_button = footer
            .widget()
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("footer action should expose row root")
            .first_child()
            .and_downcast::<gtk::Button>()
            .expect("footer action row should expose button");
        let footer_sender = sender.clone();
        footer_button.connect_clicked(move |_| {
            let _ = footer_sender.output(BatteryPopoverOutput::OpenSettings);
        });

        let model = BatteryPopover {
            popover: widgets.root.clone(),
            shell,
            hero,
            details,
            profiles,
            degraded,
            footer,
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
