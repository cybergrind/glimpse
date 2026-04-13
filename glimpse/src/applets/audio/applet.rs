use glimpse::audio::provider::{AudioEvent, AudioProvider, AudioStream, DeviceList, volume_icon};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, glib, prelude::*},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::AudioConfig;
use super::popover::{AudioPopover, AudioPopoverInit, AudioPopoverInput, AudioPopoverOutput};

pub struct Audio {
    config: AudioConfig,
    icon_name: String,
    label: String,
    tooltip: String,
    volume: u32,
    mic_muted: bool,
    visible: bool,
    latest_outputs: Option<DeviceList>,
    latest_inputs: Option<DeviceList>,
    latest_streams: Vec<AudioStream>,
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
    Reconfigure(AudioConfig),
    Scroll(f64),
    TogglePopover,
    ToggleMute,
    Popover(AudioPopoverOutput),
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
            .forward(sender.input_sender(), AudioMsg::Popover);

        let model = Audio {
            config: init.config,
            icon_name: "audio-volume-muted-symbolic".into(),
            label: String::new(),
            tooltip: String::new(),
            volume: 0,
            mic_muted: false,
            visible: false,
            latest_outputs: None,
            latest_inputs: None,
            latest_streams: Vec::new(),
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
                            if let Err(err) = AudioProvider::new().run(tx, cancel).await {
                                tracing::error!("audio provider: {err}");
                            }
                        }
                    });
                    while let Some(event) = rx.recv().await {
                        let msg = match event {
                            AudioEvent::OutputsChanged(list) => AudioMsg::OutputsChanged(list),
                            AudioEvent::InputsChanged(list) => AudioMsg::InputsChanged(list),
                            AudioEvent::StreamsChanged(streams) => {
                                AudioMsg::StreamsChanged(streams)
                            }
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

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            AudioMsg::OutputsChanged(outputs) => {
                self.latest_outputs = Some(outputs.clone());
                if let Some(device) = outputs.default_device() {
                    self.volume = device.volume;
                    self.icon_name = volume_icon(device.volume, device.muted).to_owned();
                    self.visible = true;
                    self.label = format_label(
                        &self.config.label_format,
                        device.volume,
                        &device.description,
                    );
                    self.tooltip = format_label(
                        &self.config.tooltip_format,
                        device.volume,
                        &device.description,
                    );
                }
                self.popover.emit(AudioPopoverInput::UpdateOutputs(outputs));
            }
            AudioMsg::InputsChanged(inputs) => {
                self.latest_inputs = Some(inputs.clone());
                self.mic_muted = inputs
                    .default_device()
                    .map(|device| device.muted)
                    .unwrap_or(false);
                self.popover.emit(AudioPopoverInput::UpdateInputs(inputs));
            }
            AudioMsg::StreamsChanged(streams) => {
                self.latest_streams = streams.clone();
                self.popover.emit(AudioPopoverInput::UpdateStreams(streams));
            }
            AudioMsg::Reconfigure(config) => {
                self.config = config;
                if let Some(outputs) = self.latest_outputs.clone() {
                    sender.input(AudioMsg::OutputsChanged(outputs));
                }
                if let Some(inputs) = self.latest_inputs.clone() {
                    sender.input(AudioMsg::InputsChanged(inputs));
                }
                sender.input(AudioMsg::StreamsChanged(self.latest_streams.clone()));
            }
            AudioMsg::Scroll(dy) => {
                let step = self.config.scroll_step as i64;
                let max_volume = self.config.max_volume as i64;
                let delta = if dy > 0.0 { -step } else { step };
                let new_volume = (self.volume as i64 + delta).clamp(0, max_volume) as u32;
                spawn_set_volume("@DEFAULT_SINK@".into(), new_volume, "audio: set_volume");
            }
            AudioMsg::TogglePopover => {
                self.popover.emit(AudioPopoverInput::Toggle);
            }
            AudioMsg::ToggleMute => {
                spawn_toggle_mute("@DEFAULT_SINK@".into(), "audio: toggle_mute");
            }
            AudioMsg::Popover(output) => {
                self.handle_popover_output(output);
            }
            AudioMsg::Unavailable => {
                tracing::warn!("audio applet: pactl not available");
                self.visible = false;
            }
        }
    }
}

impl Audio {
    fn handle_popover_output(&self, output: AudioPopoverOutput) {
        match output {
            AudioPopoverOutput::ToggleOutputMute => {
                spawn_toggle_mute("@DEFAULT_SINK@".into(), "audio: toggle output mute");
            }
            AudioPopoverOutput::ToggleInputMute => {
                spawn_toggle_mute("@DEFAULT_SOURCE@".into(), "audio: toggle input mute");
            }
            AudioPopoverOutput::SetOutputVolume(volume) => {
                spawn_set_volume("@DEFAULT_SINK@".into(), volume, "audio: set output volume");
            }
            AudioPopoverOutput::SetInputVolume(volume) => {
                spawn_set_volume("@DEFAULT_SOURCE@".into(), volume, "audio: set input volume");
            }
            AudioPopoverOutput::SetDefaultOutput(name) => {
                glib::spawn_future_local(async move {
                    if let Err(err) = AudioProvider::new().set_default_output(&name).await {
                        tracing::warn!("audio: set_default_output: {err}");
                    }
                });
            }
            AudioPopoverOutput::SetDefaultInput(name) => {
                glib::spawn_future_local(async move {
                    if let Err(err) = AudioProvider::new().set_default_input(&name).await {
                        tracing::warn!("audio: set_default_input: {err}");
                    }
                });
            }
            AudioPopoverOutput::ToggleStreamMute(stream_id) => {
                spawn_toggle_mute(stream_id.to_string(), "audio: stream toggle_mute");
            }
            AudioPopoverOutput::SetStreamVolume { stream_id, volume } => {
                spawn_set_volume(stream_id.to_string(), volume, "audio: stream set_volume");
            }
            AudioPopoverOutput::OpenSettings => {
                spawn_settings_command(&self.config.settings_command);
            }
        }
    }
}

fn spawn_toggle_mute(target: String, context: &'static str) {
    glib::spawn_future_local(async move {
        if let Err(err) = AudioProvider::new().toggle_mute(&target).await {
            tracing::warn!("{context}: {err}");
        }
    });
}

fn spawn_set_volume(target: String, volume: u32, context: &'static str) {
    glib::spawn_future_local(async move {
        if let Err(err) = AudioProvider::new().set_volume(&target, volume).await {
            tracing::warn!("{context}: {err}");
        }
    });
}

fn spawn_settings_command(command: &str) {
    if command.is_empty() {
        return;
    }

    let parts: Vec<&str> = command.split_whitespace().collect();
    if let Some((&program, args)) = parts.split_first() {
        let _ = std::process::Command::new(program).args(args).spawn();
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
