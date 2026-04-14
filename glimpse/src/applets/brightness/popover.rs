#![allow(unused_assignments)]

use glimpse::brightness::provider::{BrightnessDisplay, choose_primary_display};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::components::{
    footer_action::{FooterAction, FooterActionInit},
    popover_shell::{PopoverShell, PopoverShellInit, PopoverShellInput},
};
use super::components::{
    display_list::{
        BrightnessDisplayList, BrightnessDisplayListInput, BrightnessDisplayListOutput,
    },
    hero::{BrightnessHero, BrightnessHeroInput},
};

pub struct BrightnessPopover {
    popover: gtk::Popover,
    shell: Controller<PopoverShell>,
    hero: Controller<BrightnessHero>,
    display_list: Controller<BrightnessDisplayList>,
    footer: Controller<FooterAction>,
    show_settings_button: bool,
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

            #[local_ref]
            shell_widget -> gtk::Box {}
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let show_settings_button = has_settings_button(&init.settings_command);
        let shell = PopoverShell::builder()
            .launch(PopoverShellInit {
                show_footer: show_settings_button,
            })
            .detach();
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
        let footer = FooterAction::builder()
            .launch(FooterActionInit {
                title: "Display Settings".into(),
                subtitle: String::new(),
            })
            .detach();

        let shell_widget = shell.widget().clone();
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
        shell_content.append(&display_list_widget);
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
            let _ = footer_sender.output(BrightnessPopoverOutput::OpenSettings);
        });
        let model = BrightnessPopover {
            popover: widgets.root.clone(),
            shell,
            hero,
            display_list,
            footer,
            show_settings_button,
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
                self.shell
                    .emit(PopoverShellInput::SetFooterVisible(self.show_settings_button));
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
