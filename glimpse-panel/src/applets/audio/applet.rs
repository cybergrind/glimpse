use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, glib, prelude::*},
};

use super::config::AudioConfig;
use super::popover::{AudioInput, AudioOutput, AudioStream, Popover, PopoverInit, PopoverInput};

pub struct Audio {
    config: AudioConfig,
    client: Arc<Client>,
    icon_name: String,
    label: String,
    tooltip: String,
    volume: u32,
    muted: bool,
    mic_muted: bool,
    outputs: Vec<AudioOutput>,
    popover: Controller<Popover>,
}

pub struct AudioInit {
    pub config: AudioConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum AudioMsg {
    StatusUpdate(serde_json::Value),
    OutputsUpdate(Vec<AudioOutput>),
    InputsUpdate(Vec<AudioInput>),
    StreamsUpdate(Vec<AudioStream>),
    Scroll(f64),
    ShiftScroll(f64),
    TogglePopover,
    ToggleMute,
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Audio {
    type Init = AudioInit;
    type Input = AudioMsg;
    type Output = ();
    type CommandOutput = AudioMsg;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "audio",
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(AudioMsg::TogglePopover);
                }
            },

            add_controller = gtk::GestureClick {
                set_button: 2,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(AudioMsg::ToggleMute);
                }
            },


            add_controller = gtk::EventControllerScroll::new(
                gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::HORIZONTAL
            ) {
                connect_scroll[sender] => move |_ctrl, dx, dy| {
                    if dx != 0.0 {
                        // Horizontal scroll (shift+scroll on Wayland) → switch device.
                        sender.input(AudioMsg::ShiftScroll(dx));
                    } else {
                        sender.input(AudioMsg::Scroll(dy));
                    }
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

            gtk::Image {
                set_icon_name: Some("microphone-sensitivity-muted-symbolic"),
                set_pixel_size: 16,
                add_css_class: "mic-muted-indicator",
                #[watch]
                set_visible: model.mic_muted,
            },

            gtk::Label {
                #[watch]
                set_label: &model.label,
                add_css_class: "audio-label",
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
        let popover = Popover::builder()
            .launch(PopoverInit {
                parent: root.clone(),
                client: init.client.clone(),
                config: init.config.clone(),
            })
            .detach();

        let model = Audio {
            config: init.config,
            client: init.client.clone(),
            icon_name: "audio-volume-muted-symbolic".into(),
            label: String::new(),
            tooltip: String::new(),
            volume: 0,
            muted: false,
            mic_muted: false,
            outputs: Vec::new(),
            popover,
        };

        let client = init.client;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("audio applet: subscribing");
                    let mut status_sub = match client.subscribe("audio.status").await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("audio: subscribe failed: {e}");
                            let _ = out.send(AudioMsg::Unavailable);
                            return;
                        }
                    };
                    let mut outputs_sub = client.subscribe("audio.outputs").await.ok();
                    let mut inputs_sub = client.subscribe("audio.inputs").await.ok();
                    let mut streams_sub = client.subscribe("audio.streams").await.ok();

                    loop {
                        tokio::select! {
                            Some(ev) = status_sub.next() => {
                                let _ = out.send(AudioMsg::StatusUpdate(ev.data));
                            }
                            Some(ev) = async {
                                match &mut outputs_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                if let Ok(outputs) = serde_json::from_value(ev.data) {
                                    let _ = out.send(AudioMsg::OutputsUpdate(outputs));
                                }
                            }
                            Some(ev) = async {
                                match &mut inputs_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                if let Ok(inputs) = serde_json::from_value(ev.data) {
                                    let _ = out.send(AudioMsg::InputsUpdate(inputs));
                                }
                            }
                            Some(ev) = async {
                                match &mut streams_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                if let Ok(streams) = serde_json::from_value(ev.data) {
                                    let _ = out.send(AudioMsg::StreamsUpdate(streams));
                                }
                            }
                            else => break,
                        }
                    }
                    let _ = out.send(AudioMsg::Unavailable);
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            AudioMsg::StatusUpdate(data) => {
                tracing::info!(volume = %data["volume"], muted = %data["muted"], "audio applet: status update");
                self.volume = data["volume"].as_u64().unwrap_or(0) as u32;
                self.muted = data["muted"].as_bool().unwrap_or(false);
                self.icon_name = data["icon_name"]
                    .as_str()
                    .unwrap_or("audio-volume-muted-symbolic")
                    .to_owned();

                let device = data["default_output"].as_str().unwrap_or("");

                self.label = if self.config.label_format.is_empty() {
                    String::new()
                } else {
                    self.config
                        .label_format
                        .replace("{volume}", &self.volume.to_string())
                        .replace("{device}", device)
                };

                self.tooltip = if self.config.tooltip_format.is_empty() {
                    String::new()
                } else {
                    self.config
                        .tooltip_format
                        .replace("{volume}", &self.volume.to_string())
                        .replace("{device}", device)
                        .trim_end_matches([' ', ',', '-', '—'])
                        .to_owned()
                };

                self.popover.emit(PopoverInput::UpdateStatus {
                    icon: self.icon_name.clone(),
                    description: device.to_owned(),
                    volume: self.volume,
                    muted: self.muted,
                });
            }
            AudioMsg::OutputsUpdate(outputs) => {
                self.outputs = outputs.clone();
                self.popover.emit(PopoverInput::UpdateOutputs(outputs));
            }
            AudioMsg::InputsUpdate(inputs) => {
                self.mic_muted = inputs
                    .iter()
                    .find(|i| i.is_default)
                    .map(|i| i.muted)
                    .unwrap_or(false);
                self.popover.emit(PopoverInput::UpdateInputs(inputs));
            }
            AudioMsg::StreamsUpdate(streams) => {
                self.popover.emit(PopoverInput::UpdateStreams(streams));
            }
            AudioMsg::Scroll(dy) => {
                let step = self.config.scroll_step as i64;
                let max = self.config.max_volume as i64;
                let delta = if dy > 0.0 { -step } else { step };
                let new_vol = (self.volume as i64 + delta).clamp(0, max) as u32;
                let client = self.client.clone();
                glib::spawn_future_local(async move {
                    let _ = client
                        .call("audio.set_volume", serde_json::json!({"volume": new_vol}))
                        .await;
                });
            }
            AudioMsg::ShiftScroll(dy) => {
                // Cycle output device.
                if self.outputs.len() < 2 {
                    return;
                }
                let current_idx = self.outputs.iter().position(|o| o.is_default).unwrap_or(0);
                let next_idx = if dy > 0.0 {
                    (current_idx + 1) % self.outputs.len()
                } else {
                    (current_idx + self.outputs.len() - 1) % self.outputs.len()
                };
                let name = self.outputs[next_idx].name.clone();
                let client = self.client.clone();
                glib::spawn_future_local(async move {
                    let _ = client
                        .call(
                            "audio.set_default_output",
                            serde_json::json!({"name": name}),
                        )
                        .await;
                });
            }
            AudioMsg::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            AudioMsg::ToggleMute => {
                let client = self.client.clone();
                glib::spawn_future_local(async move {
                    let _ = client.call("audio.set_mute", serde_json::json!({})).await;
                });
            }
            AudioMsg::Unavailable => {
                tracing::warn!("audio applet: daemon unavailable");
            }
        }
    }
}
