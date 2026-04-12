#![allow(unused_assignments)]

use glimpse::providers::brightness::{BrightnessDisplay, choose_primary_display};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::{
    display_list::{
        BrightnessDisplayList, BrightnessDisplayListInput, BrightnessDisplayListOutput,
    },
    hero::{BrightnessHero, BrightnessHeroInput},
};

pub struct BrightnessPopover {
    popover: gtk::Popover,
    hero: Controller<BrightnessHero>,
    display_list: Controller<BrightnessDisplayList>,
}

pub struct BrightnessPopoverInit {
    pub parent: gtk::Box,
    pub settings_command: String,
}

#[derive(Debug)]
pub enum BrightnessPopoverInput {
    Toggle,
    UpdateDisplays(Vec<BrightnessDisplay>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrightnessPopoverOutput {
    Opened,
    Closed,
    SetDisplayPercent { display_id: String, percent: u8 },
    OpenSettings,
}

#[relm4::component(pub)]
impl SimpleComponent for BrightnessPopover {
    type Init = BrightnessPopoverInit;
    type Input = BrightnessPopoverInput;
    type Output = BrightnessPopoverOutput;

    view! {
        root = gtk::Popover {
            set_autohide: true,
            add_css_class: "brightness-popover",

            connect_show[sender] => move |_| {
                let _ = sender.output(BrightnessPopoverOutput::Opened);
            },

            connect_closed[sender] => move |_| {
                let _ = sender.output(BrightnessPopoverOutput::Closed);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,

                #[local_ref]
                hero_widget -> gtk::Box {},

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[local_ref]
                display_list_widget -> gtk::Box {},

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_visible: has_settings_button(&init.settings_command),
                },

                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "settings-btn",
                    set_visible: has_settings_button(&init.settings_command),
                    connect_clicked[sender] => move |_| {
                        let _ = sender.output(BrightnessPopoverOutput::OpenSettings);
                    },

                    gtk::Label {
                        set_label: "Display Settings",
                    }
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let hero = BrightnessHero::builder().launch(()).detach();
        let hero_widget = hero.widget().clone();

        let display_list =
            BrightnessDisplayList::builder()
                .launch(())
                .forward(sender.output_sender(), |output| match output {
                    BrightnessDisplayListOutput::SetDisplayPercent {
                        display_id,
                        percent,
                    } => BrightnessPopoverOutput::SetDisplayPercent {
                        display_id,
                        percent,
                    },
                });
        let display_list_widget = display_list.widget().clone();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        let model = BrightnessPopover {
            popover: widgets.root.clone(),
            hero,
            display_list,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            BrightnessPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            BrightnessPopoverInput::UpdateDisplays(displays) => {
                self.hero.emit(BrightnessHeroInput::Update(
                    choose_primary_display(&displays).cloned(),
                ));
                self.display_list
                    .emit(BrightnessDisplayListInput::Update(displays));
            }
        }
    }
}

fn has_settings_button(command: &str) -> bool {
    !command.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::has_settings_button;

    #[test]
    fn settings_button_requires_non_whitespace_command() {
        assert!(!has_settings_button(""));
        assert!(!has_settings_button("   \t"));
        assert!(has_settings_button("gnome-control-center display"));
    }
}
