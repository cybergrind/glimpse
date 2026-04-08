use glimpse::providers::battery::BatteryStatus;
use glimpse::providers::power::PowerProfiles;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::degraded::{DegradedWarning, DegradedWarningInput};
use super::components::details::{BatteryDetails, BatteryDetailsInput};
use super::components::hero::{BatteryHero, BatteryHeroInput};
use super::components::profiles::{PowerProfileList, PowerProfileListInit, PowerProfileListInput};

pub struct BatteryPopover {
    popover: gtk::Popover,
    hero: Controller<BatteryHero>,
    details: Controller<BatteryDetails>,
    profiles: Controller<PowerProfileList>,
    degraded: Controller<DegradedWarning>,
}

pub struct BatteryPopoverInit {
    pub parent: gtk::Box,
    pub conn: zbus::Connection,
    pub settings_command: String,
}

#[derive(Debug)]
pub enum BatteryPopoverInput {
    Toggle,
    UpdateStatus(BatteryStatus),
    UpdateProfiles(PowerProfiles),
}

impl SimpleComponent for BatteryPopover {
    type Init = BatteryPopoverInit;
    type Input = BatteryPopoverInput;
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
        root.add_css_class("battery-popover");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        let hero = BatteryHero::builder().launch(()).detach();
        vbox.append(hero.widget());

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let details = BatteryDetails::builder().launch(()).detach();
        vbox.append(details.widget());

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let profiles = PowerProfileList::builder()
            .launch(PowerProfileListInit { conn: init.conn })
            .detach();
        vbox.append(profiles.widget());

        let degraded = DegradedWarning::builder().launch(()).detach();
        vbox.append(degraded.widget());

        if !init.settings_command.is_empty() {
            vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            let cmd = init.settings_command;
            let lbl = gtk::Label::new(Some("Power Settings"));
            lbl.set_halign(gtk::Align::Start);
            let btn = gtk::Button::new();
            btn.set_child(Some(&lbl));
            btn.add_css_class("flat");
            btn.add_css_class("settings-btn");
            btn.connect_clicked(move |_| {
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if let Some((&prog, args)) = parts.split_first() {
                    let _ = std::process::Command::new(prog).args(args).spawn();
                }
            });
            vbox.append(&btn);
        }

        root.set_child(Some(&vbox));

        let model = BatteryPopover {
            popover: root.clone(),
            hero,
            details,
            profiles,
            degraded,
        };
        ComponentParts { model, widgets: () }
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
