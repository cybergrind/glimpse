use glimpse::audio::provider::{AudioStream, DeviceList};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::AudioConfig;
use super::components::devices::{
    DeviceSection, DeviceSectionInit, DeviceSectionInput, DeviceSectionOutput,
};
use super::components::hero::{AudioHero, AudioHeroInput};
use super::components::streams::{StreamList, StreamListInit, StreamListInput, StreamListOutput};
use super::components::volume::{
    VolumeSection, VolumeSectionInit, VolumeSectionInput, VolumeSectionOutput,
};
use crate::components::{
    footer_action::{FooterAction, FooterActionInit},
    popover_shell::{PopoverShell, PopoverShellInit},
};

pub struct AudioPopover {
    popover: gtk::Popover,
    shell: Controller<PopoverShell>,
    hero: Controller<AudioHero>,
    volume: Controller<VolumeSection>,
    devices: Controller<DeviceSection>,
    streams: Controller<StreamList>,
    footer: Controller<FooterAction>,
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
                show_footer: !init.config.settings_command.is_empty(),
            })
            .detach();
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
        let footer = FooterAction::builder()
            .launch(FooterActionInit {
                title: "Audio Settings".into(),
                subtitle: String::new(),
            })
            .detach();

        let shell_widget = shell.widget().clone();
        let hero_widget = hero.widget().clone();
        let volume_widget = volume.widget().clone();
        let devices_widget = devices.widget().clone();
        let streams_widget = streams.widget().clone();
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
        shell_content.append(&volume_widget);
        shell_content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        shell_content.append(&devices_widget);
        shell_content.append(&streams_widget);
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
            let _ = footer_sender.output(AudioPopoverOutput::OpenSettings);
        });

        let model = AudioPopover {
            popover: widgets.root.clone(),
            shell,
            hero,
            volume,
            devices,
            streams,
            footer,
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
