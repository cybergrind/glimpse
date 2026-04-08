use glimpse::providers::audio::{AudioEvent, AudioProvider, AudioStream, DeviceList, volume_icon};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, glib, prelude::*},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::config::AudioConfig;
use super::popover::{AudioPopover, AudioPopoverInit, AudioPopoverInput};

pub struct Audio {
    config: AudioConfig,
    icon_name: String,
    label: String,
    tooltip: String,
    volume: u32,
    muted: bool,
    mic_muted: bool,
    visible: bool,
    popover: Controller<AudioPopover>,
}

pub struct AudioInit {
    pub config: AudioConfig,
}

#[derive(Debug)]
pub enum AudioMsg {
    OutputsChanged(DeviceList),
    InputsChanged(DeviceList),
    StreamsChanged(Vec<AudioStream>),
    Scroll(f64),
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
            set_visible: model.visible,
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
                gtk::EventControllerScrollFlags::VERTICAL
            ) {
                connect_scroll[sender] => move |_, _dx, dy| {
                    sender.input(AudioMsg::Scroll(dy));
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
                set_visible: model.config.show_mic_indicator && model.mic_muted,
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
        let popover = AudioPopover::builder()
            .launch(AudioPopoverInit {
                parent: root.clone(),
                config: init.config.clone(),
            })
            .detach();

        let model = Audio {
            config: init.config,
            icon_name: "audio-volume-muted-symbolic".into(),
            label: String::new(),
            tooltip: String::new(),
            volume: 0,
            muted: false,
            mic_muted: false,
            visible: false,
            popover,
        };

        sender.command(|out, shutdown| {
            shutdown
                .register(async move {
                    let cancel = CancellationToken::new();
                    let (tx, mut rx) = mpsc::channel::<AudioEvent>(8);
                    tokio::spawn({
                        let cancel = cancel.clone();
                        async move {
                            if let Err(e) = AudioProvider::new().run(tx, cancel).await {
                                tracing::error!("audio provider: {e}");
                            }
                        }
                    });
                    while let Some(event) = rx.recv().await {
                        let msg = match event {
                            AudioEvent::OutputsChanged(list) => AudioMsg::OutputsChanged(list),
                            AudioEvent::InputsChanged(list) => AudioMsg::InputsChanged(list),
                            AudioEvent::StreamsChanged(s) => AudioMsg::StreamsChanged(s),
                            AudioEvent::Unavailable => AudioMsg::Unavailable,
                        };
                        let _ = out.send(msg);
                    }
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
            AudioMsg::OutputsChanged(outputs) => {
                if let Some(d) = outputs.default_device() {
                    self.volume = d.volume;
                    self.muted = d.muted;
                    self.icon_name = volume_icon(d.volume, d.muted).to_owned();
                    self.visible = true;
                    self.label = format_label(&self.config.label_format, d.volume, &d.description);
                    self.tooltip =
                        format_label(&self.config.tooltip_format, d.volume, &d.description);
                }
                self.popover.emit(AudioPopoverInput::UpdateOutputs(outputs));
            }
            AudioMsg::InputsChanged(inputs) => {
                self.mic_muted = inputs.default_device().map(|d| d.muted).unwrap_or(false);
                self.popover.emit(AudioPopoverInput::UpdateInputs(inputs));
            }
            AudioMsg::StreamsChanged(streams) => {
                self.popover.emit(AudioPopoverInput::UpdateStreams(streams));
            }
            AudioMsg::Scroll(dy) => {
                let step = self.config.scroll_step as i64;
                let max = self.config.max_volume as i64;
                let delta = if dy > 0.0 { -step } else { step };
                let new_vol = (self.volume as i64 + delta).clamp(0, max) as u32;
                glib::spawn_future_local(async move {
                    if let Err(e) = AudioProvider::new()
                        .set_volume("@DEFAULT_SINK@", new_vol)
                        .await
                    {
                        tracing::warn!("audio: set_volume: {e}");
                    }
                });
            }
            AudioMsg::TogglePopover => {
                self.popover.emit(AudioPopoverInput::Toggle);
            }
            AudioMsg::ToggleMute => {
                glib::spawn_future_local(async move {
                    if let Err(e) = AudioProvider::new().toggle_mute("@DEFAULT_SINK@").await {
                        tracing::warn!("audio: toggle_mute: {e}");
                    }
                });
            }
            AudioMsg::Unavailable => {
                tracing::warn!("audio applet: pactl not available");
                self.visible = false;
            }
        }
    }
}

fn format_label(template: &str, volume: u32, device: &str) -> String {
    if template.is_empty() {
        return String::new();
    }
    template
        .replace("{volume}", &volume.to_string())
        .replace("{device}", device)
        .trim_end_matches([' ', ',', '-', '—'])
        .to_owned()
}
