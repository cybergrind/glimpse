use glimpse::providers::audio::{AudioStream, DeviceList};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::devices::{DeviceSection, DeviceSectionInit, DeviceSectionInput};
use super::components::hero::{AudioHero, AudioHeroInput};
use super::components::streams::{StreamList, StreamListInit, StreamListInput};
use super::components::volume::{VolumeSection, VolumeSectionInit, VolumeSectionInput};
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

impl SimpleComponent for AudioPopover {
    type Init = AudioPopoverInit;
    type Input = AudioPopoverInput;
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
        root.add_css_class("audio-popover");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        let hero = AudioHero::builder().launch(()).detach();
        vbox.append(hero.widget());

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let volume = VolumeSection::builder()
            .launch(VolumeSectionInit { max_volume: init.config.max_volume as f64 })
            .detach();
        vbox.append(volume.widget());

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let devices = DeviceSection::builder()
            .launch(DeviceSectionInit)
            .detach();
        vbox.append(devices.widget());

        let streams = StreamList::builder()
            .launch(StreamListInit {
                max_vol: init.config.max_volume as f64,
                show_streams: init.config.show_streams,
            })
            .detach();
        vbox.append(streams.widget());

        if !init.config.settings_command.is_empty() {
            vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            let cmd = init.config.settings_command.clone();
            let lbl = gtk::Label::new(Some("Audio Settings"));
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

        let model = AudioPopover { popover: root.clone(), hero, volume, devices, streams };
        ComponentParts { model, widgets: () }
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
                if let Some(d) = outputs.default_device() {
                    use glimpse::providers::audio::volume_icon;
                    let subtitle = if d.muted {
                        format!("{} — muted", d.description)
                    } else {
                        format!("{} — {}%", d.description, d.volume)
                    };
                    self.hero.emit(AudioHeroInput::Update {
                        icon_name: volume_icon(d.volume, d.muted).to_owned(),
                        subtitle,
                    });
                }
                self.volume.emit(VolumeSectionInput::UpdateOutputs(outputs.clone()));
                self.devices.emit(DeviceSectionInput::UpdateOutputs(outputs));
            }
            AudioPopoverInput::UpdateInputs(inputs) => {
                self.volume.emit(VolumeSectionInput::UpdateInputs(inputs.clone()));
                self.devices.emit(DeviceSectionInput::UpdateInputs(inputs));
            }
            AudioPopoverInput::UpdateStreams(streams) => {
                self.streams.emit(StreamListInput::Update(streams));
            }
        }
    }
}
