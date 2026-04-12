use glimpse::{
    brightness::{
        BrightnessServiceHandle,
        protocol::{BrightnessServiceCommand, BrightnessServiceState},
    },
    brightness::provider::{BrightnessDisplay, choose_primary_display},
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, glib, prelude::*},
};

use super::BrightnessConfig;
use super::popover::{
    BrightnessPopover, BrightnessPopoverInit, BrightnessPopoverInput, BrightnessPopoverOutput,
};

pub struct Brightness {
    config: BrightnessConfig,
    service: BrightnessServiceHandle,
    icon_name: String,
    label: String,
    tooltip: String,
    hidden: bool,
    displays: Vec<BrightnessDisplay>,
    popover: Controller<BrightnessPopover>,
}

pub struct BrightnessInit {
    pub config: BrightnessConfig,
    pub service: BrightnessServiceHandle,
}

#[derive(Debug, Clone)]
pub enum BrightnessMsg {
    ServiceState(BrightnessServiceState),
    Scroll(f64),
    TogglePopover,
    Popover(BrightnessPopoverOutput),
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Brightness {
    type Init = BrightnessInit;
    type Input = BrightnessMsg;
    type Output = ();
    type CommandOutput = BrightnessMsg;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "brightness",

            #[watch]
            set_visible: !model.hidden,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(BrightnessMsg::TogglePopover);
                }
            },

            add_controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL) {
                connect_scroll[sender] => move |_, _dx, dy| {
                    sender.input(BrightnessMsg::Scroll(dy));
                    glib::Propagation::Stop
                }
            },

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 16,
                #[watch]
                set_visible: model.config.show_icon,
            },

            gtk::Label {
                #[watch]
                set_label: &model.label,
                add_css_class: "brightness-label",
                #[watch]
                set_visible: !model.label.is_empty(),
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = BrightnessPopover::builder()
            .launch(BrightnessPopoverInit {
                parent: root.clone(),
                settings_command: init.config.settings_command.clone(),
            })
            .forward(sender.input_sender(), BrightnessMsg::Popover);

        let model = Brightness {
            config: init.config,
            service: init.service.clone(),
            icon_name: "display-brightness-off-symbolic".into(),
            label: String::new(),
            tooltip: String::new(),
            hidden: true,
            displays: Vec::new(),
            popover,
        };

        let service = init.service;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("brightness applet: subscribing to brightness service");
                    let mut state_rx = service.subscribe();
                    let _ = out.send(BrightnessMsg::ServiceState(state_rx.borrow().clone()));

                    loop {
                        if state_rx.changed().await.is_err() {
                            break;
                        }
                        let _ = out.send(BrightnessMsg::ServiceState(state_rx.borrow().clone()));
                    }

                    let _ = out.send(BrightnessMsg::Unavailable);
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(message, sender, root);
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            BrightnessMsg::ServiceState(state) => {
                self.displays = state.snapshot.displays;
                if let Some(primary) = choose_primary_display(&self.displays) {
                    self.hidden = false;
                    self.icon_name = applet_icon_name().into();
                    self.label = format_label(&self.config.label_format, primary);
                    self.tooltip = format!("{} — {}%", primary.name, primary.percentage);
                } else {
                    self.hidden = self.config.hide_when_unavailable;
                    self.label.clear();
                    self.tooltip.clear();
                    self.icon_name = "display-brightness-off-symbolic".into();
                }

                self.popover.emit(BrightnessPopoverInput::UpdateDisplays(
                    self.displays.clone(),
                ));
            }
            BrightnessMsg::Scroll(dy) => {
                let Some(primary) = choose_primary_display(&self.displays) else {
                    return;
                };
                let delta_percent = if dy < 0.0 {
                    self.config.scroll_step as i32
                } else {
                    -(self.config.scroll_step as i32)
                };
                self.send_command(
                    sender,
                    BrightnessServiceCommand::AdjustDisplayPercent {
                        display_id: primary.id.clone(),
                        delta_percent,
                    },
                );
            }
            BrightnessMsg::TogglePopover => {
                self.popover.emit(BrightnessPopoverInput::Toggle);
            }
            BrightnessMsg::Popover(output) => {
                self.handle_popover_output(output, sender);
            }
            BrightnessMsg::Unavailable => {
                self.displays.clear();
                self.hidden = self.config.hide_when_unavailable;
                self.label.clear();
                self.tooltip.clear();
                self.icon_name = "display-brightness-off-symbolic".into();
                self.popover
                    .emit(BrightnessPopoverInput::UpdateDisplays(Vec::new()));
            }
        }
    }
}

impl Brightness {
    fn handle_popover_output(
        &self,
        output: BrightnessPopoverOutput,
        sender: ComponentSender<Brightness>,
    ) {
        match output {
            BrightnessPopoverOutput::Opened => {
                self.send_command(sender, BrightnessServiceCommand::PopoverOpened);
            }
            BrightnessPopoverOutput::Closed => {
                self.send_command(sender, BrightnessServiceCommand::PopoverClosed);
            }
            BrightnessPopoverOutput::SetDisplayPercent {
                display_id,
                percent,
            } => {
                self.send_command(
                    sender,
                    BrightnessServiceCommand::SetDisplayPercent {
                        display_id,
                        percent,
                    },
                );
            }
            BrightnessPopoverOutput::OpenSettings => {
                spawn_settings_command(&self.config.settings_command);
            }
        }
    }

    fn send_command(&self, sender: ComponentSender<Brightness>, command: BrightnessServiceCommand) {
        let service = self.service.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    if let Err(error) = service.send(command).await {
                        tracing::warn!(error = %error, "brightness applet: failed to send service command");
                    }
                })
                .drop_on_shutdown()
        });
    }
}

pub(super) fn applet_icon_name() -> &'static str {
    "display-brightness-symbolic"
}

fn format_label(template: &str, display: &BrightnessDisplay) -> String {
    if template.is_empty() {
        return String::new();
    }

    template
        .replace("{percentage}", &display.percentage.to_string())
        .replace("{display}", &display.name)
        .trim_end_matches([' ', ',', '-', '—'])
        .to_owned()
}

fn spawn_settings_command(command: &str) {
    if command.trim().is_empty() {
        return;
    }

    let command = command.to_owned();
    if let Ok(mut child) = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{applet_icon_name, format_label};
    use glimpse::brightness::provider::{BrightnessBackend, BrightnessDisplay};

    #[test]
    fn summary_icon_name_uses_percentage_bands() {
        fn summary_icon_name(percentage: u8) -> &'static str {
            match percentage {
                0 => "display-brightness-off-symbolic",
                1..=33 => "display-brightness-low-symbolic",
                34..=66 => "display-brightness-medium-symbolic",
                _ => "display-brightness-high-symbolic",
            }
        }

        assert_eq!(summary_icon_name(0), "display-brightness-off-symbolic");
        assert_eq!(summary_icon_name(20), "display-brightness-low-symbolic");
        assert_eq!(summary_icon_name(50), "display-brightness-medium-symbolic");
        assert_eq!(summary_icon_name(90), "display-brightness-high-symbolic");
    }

    #[test]
    fn applet_icon_name_is_stable() {
        assert_eq!(applet_icon_name(), "display-brightness-symbolic");
    }

    #[test]
    fn format_label_supports_display_name_and_percentage() {
        let display = BrightnessDisplay {
            id: "backlight:intel".into(),
            name: "Laptop".into(),
            backend: BrightnessBackend::Backlight,
            current: 1200,
            max: 2000,
            percentage: 60,
            is_internal: true,
            is_primary: true,
            available: true,
        };

        assert_eq!(
            format_label("{display}: {percentage}%", &display),
            "Laptop: 60%"
        );
    }
}
