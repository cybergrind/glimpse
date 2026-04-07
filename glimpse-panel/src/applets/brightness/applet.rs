use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, glib, prelude::*},
};
use serde::Deserialize;

use super::config::BrightnessConfig;
use super::popover::{BrightnessPopover, BrightnessPopoverInit, BrightnessPopoverInput};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct BrightnessDisplay {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub current: u32,
    pub max: u32,
    pub percentage: u32,
    pub is_internal: bool,
    pub is_primary: bool,
    pub available: bool,
}

#[derive(Debug, Deserialize)]
struct BrightnessDisplaysPayload {
    displays: Vec<BrightnessDisplay>,
}

pub struct Brightness {
    config: BrightnessConfig,
    client: Arc<Client>,
    icon_name: String,
    label: String,
    tooltip: String,
    hidden: bool,
    displays: Vec<BrightnessDisplay>,
    popover: Controller<BrightnessPopover>,
}

pub struct BrightnessInit {
    pub config: BrightnessConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum BrightnessMsg {
    DisplaysUpdate(Vec<BrightnessDisplay>),
    Scroll(f64),
    TogglePopover,
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
                client: init.client.clone(),
                settings_command: init.config.settings_command.clone(),
            })
            .detach();

        let model = Brightness {
            config: init.config,
            client: init.client.clone(),
            icon_name: "display-brightness-off-symbolic".into(),
            label: String::new(),
            tooltip: String::new(),
            hidden: true,
            displays: Vec::new(),
            popover,
        };

        let client = init.client;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("brightness applet: subscribing");
                    let mut sub = match client.subscribe("brightness.displays").await {
                        Ok(subscription) => subscription,
                        Err(error) => {
                            tracing::error!("brightness: subscribe failed: {error}");
                            let _ = out.send(BrightnessMsg::Unavailable);
                            return;
                        }
                    };

                    loop {
                        match sub.next().await {
                            Some(event) => {
                                match serde_json::from_value::<BrightnessDisplaysPayload>(
                                    event.data,
                                ) {
                                    Ok(payload) => {
                                        let _ = out
                                            .send(BrightnessMsg::DisplaysUpdate(payload.displays));
                                    }
                                    Err(error) => {
                                        tracing::warn!(%error, "brightness: invalid payload");
                                    }
                                }
                            }
                            None => {
                                let _ = out.send(BrightnessMsg::Unavailable);
                                break;
                            }
                        }
                    }
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

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            BrightnessMsg::DisplaysUpdate(displays) => {
                self.displays = displays;
                if let Some(primary) = choose_primary_display(&self.displays) {
                    self.hidden = false;
                    self.icon_name = summary_icon_name(primary.percentage).into();
                    self.label = self
                        .config
                        .label_format
                        .replace("{percentage}", &primary.percentage.to_string())
                        .replace("{display}", &primary.name);
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
                let delta = if dy < 0.0 {
                    self.config.scroll_step as i32
                } else {
                    -(self.config.scroll_step as i32)
                };
                let client = self.client.clone();
                let display_id = primary.id.clone();
                glib::spawn_future_local(async move {
                    let _ = client
                        .call(
                            "brightness.set_relative",
                            serde_json::json!({
                                "display_id": display_id,
                                "delta": delta,
                                "is_percentage": true
                            }),
                        )
                        .await;
                });
            }
            BrightnessMsg::TogglePopover => {
                self.popover.emit(BrightnessPopoverInput::Toggle);
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

pub fn choose_primary_display(displays: &[BrightnessDisplay]) -> Option<&BrightnessDisplay> {
    displays
        .iter()
        .find(|display| display.is_primary && display.available)
        .or_else(|| {
            displays
                .iter()
                .find(|display| display.is_internal && display.available)
        })
        .or_else(|| displays.iter().find(|display| display.available))
}

pub fn summary_icon_name(percentage: u32) -> &'static str {
    match percentage {
        0 => "display-brightness-off-symbolic",
        1..=33 => "display-brightness-low-symbolic",
        34..=66 => "display-brightness-medium-symbolic",
        _ => "display-brightness-high-symbolic",
    }
}

#[cfg(test)]
mod tests {
    use super::{BrightnessDisplay, choose_primary_display, summary_icon_name};

    #[test]
    fn choose_primary_display_prefers_internal_display() {
        let displays = vec![
            BrightnessDisplay {
                id: "ddc:1".into(),
                name: "Dell".into(),
                backend: "ddc".into(),
                current: 40,
                max: 100,
                percentage: 40,
                is_internal: false,
                is_primary: false,
                available: true,
            },
            BrightnessDisplay {
                id: "backlight:intel".into(),
                name: "Laptop".into(),
                backend: "internal".into(),
                current: 1200,
                max: 2000,
                percentage: 60,
                is_internal: true,
                is_primary: true,
                available: true,
            },
        ];

        let primary = choose_primary_display(&displays).expect("primary display");
        assert_eq!(primary.id, "backlight:intel");
    }

    #[test]
    fn summary_icon_name_uses_percentage_bands() {
        assert_eq!(summary_icon_name(0), "display-brightness-off-symbolic");
        assert_eq!(summary_icon_name(20), "display-brightness-low-symbolic");
        assert_eq!(summary_icon_name(50), "display-brightness-medium-symbolic");
        assert_eq!(summary_icon_name(90), "display-brightness-high-symbolic");
    }
}
