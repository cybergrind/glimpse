use glimpse::audio::provider::{AudioStream, DeviceList};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::devices::{
    DeviceSection, DeviceSectionInit, DeviceSectionInput, DeviceSectionOutput,
};
use super::components::hero::{AudioHero, AudioHeroInput};
use super::components::streams::{StreamList, StreamListInit, StreamListInput, StreamListOutput};
use super::components::volume::{
    VolumeSection, VolumeSectionInit, VolumeSectionInput, VolumeSectionOutput,
};
use super::config::AudioConfig;

pub struct AudioPopover {
    popover: gtk::Popover,
    hero: Controller<AudioHero>,
    volume: Controller<VolumeSection>,
    devices: Controller<DeviceSection>,
    streams: Controller<StreamList>,
}

pub struct AudioPopoverInit {
    pub parent: gtk::Box,
    pub config: AudioConfig,
}

#[derive(Debug)]
pub enum AudioPopoverInput {
    Toggle,
    UpdateOutputs(DeviceList),
    UpdateInputs(DeviceList),
    UpdateStreams(Vec<AudioStream>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AudioPopoverOutput {
    ToggleOutputMute,
    ToggleInputMute,
    SetOutputVolume(u32),
    SetInputVolume(u32),
    SetDefaultOutput(String),
    SetDefaultInput(String),
    ToggleStreamMute(u64),
    SetStreamVolume { stream_id: u64, volume: u32 },
    OpenSettings,
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for AudioPopover {
    type Init = AudioPopoverInit;
    type Input = AudioPopoverInput;
    type Output = AudioPopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "audio-popover",

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: false,
                set_overflow: gtk::Overflow::Hidden,

                #[local_ref]
                hero_widget -> gtk::Box {},

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[local_ref]
                volume_widget -> gtk::Box {},

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[local_ref]
                devices_widget -> gtk::Box {},

                #[local_ref]
                streams_widget -> gtk::Box {},

                #[name(settings_sep)]
                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_visible: !init.config.settings_command.is_empty(),
                },

                #[name(settings_button)]
                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "settings-btn",
                    set_visible: !init.config.settings_command.is_empty(),
                    connect_clicked[sender] => move |_| {
                        let _ = sender.output(AudioPopoverOutput::OpenSettings);
                    },

                    gtk::Label {
                        set_label: "Audio Settings",
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
        let hero = AudioHero::builder().launch(()).detach();
        let volume = VolumeSection::builder()
            .launch(VolumeSectionInit {
                max_volume: init.config.max_volume as f64,
            })
            .forward(sender.output_sender(), map_volume_output);
        let devices = DeviceSection::builder()
            .launch(DeviceSectionInit)
            .forward(sender.output_sender(), map_device_output);
        let streams = StreamList::builder()
            .launch(StreamListInit {
                max_volume: init.config.max_volume as f64,
                show_streams: init.config.show_streams,
            })
            .forward(sender.output_sender(), map_stream_output);

        let hero_widget = hero.widget().clone();
        let volume_widget = volume.widget().clone();
        let devices_widget = devices.widget().clone();
        let streams_widget = streams.widget().clone();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let model = AudioPopover {
            popover: widgets.root.clone(),
            hero,
            volume,
            devices,
            streams,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            AudioPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            AudioPopoverInput::UpdateOutputs(outputs) => {
                if let Some(device) = outputs.default_device() {
                    use glimpse::audio::provider::volume_icon;

                    let subtitle = if device.muted {
                        format!("{} — muted", device.description)
                    } else {
                        format!("{} — {}%", device.description, device.volume)
                    };

                    self.hero.emit(AudioHeroInput::Update {
                        icon_name: volume_icon(device.volume, device.muted).to_owned(),
                        subtitle,
                    });
                }

                self.volume
                    .emit(VolumeSectionInput::UpdateOutputs(outputs.clone()));
                self.devices
                    .emit(DeviceSectionInput::UpdateOutputs(outputs));
            }
            AudioPopoverInput::UpdateInputs(inputs) => {
                self.volume
                    .emit(VolumeSectionInput::UpdateInputs(inputs.clone()));
                self.devices.emit(DeviceSectionInput::UpdateInputs(inputs));
            }
            AudioPopoverInput::UpdateStreams(streams) => {
                self.streams.emit(StreamListInput::Update(streams));
            }
        }
    }
}

fn map_volume_output(output: VolumeSectionOutput) -> AudioPopoverOutput {
    match output {
        VolumeSectionOutput::ToggleOutputMute => AudioPopoverOutput::ToggleOutputMute,
        VolumeSectionOutput::ToggleInputMute => AudioPopoverOutput::ToggleInputMute,
        VolumeSectionOutput::SetOutputVolume(volume) => AudioPopoverOutput::SetOutputVolume(volume),
        VolumeSectionOutput::SetInputVolume(volume) => AudioPopoverOutput::SetInputVolume(volume),
    }
}

fn map_device_output(output: DeviceSectionOutput) -> AudioPopoverOutput {
    match output {
        DeviceSectionOutput::SetDefaultOutput(name) => AudioPopoverOutput::SetDefaultOutput(name),
        DeviceSectionOutput::SetDefaultInput(name) => AudioPopoverOutput::SetDefaultInput(name),
    }
}

fn map_stream_output(output: StreamListOutput) -> AudioPopoverOutput {
    match output {
        StreamListOutput::ToggleMute(stream_id) => AudioPopoverOutput::ToggleStreamMute(stream_id),
        StreamListOutput::SetVolume { stream_id, volume } => {
            AudioPopoverOutput::SetStreamVolume { stream_id, volume }
        }
    }
}
