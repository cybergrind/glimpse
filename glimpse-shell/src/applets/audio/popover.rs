#![allow(unused_assignments)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use crate::{
    components::{
        animated_popover::AnimatedPopover,
        collapsible_section::CollapsibleSectionView,
        device_list::{DeviceList, DeviceListInit, DeviceListInput, DeviceListItem},
        hero::HeroView,
        popover_shell::PopoverShell,
    },
    services::audio::{AudioDevice, AudioStream, Command, State, volume_icon},
};

pub struct Popover {
    animation: AnimatedPopover,
    state: State,
    max_volume: u32,
    show_streams: bool,
    streams_expanded: bool,
    updating_output_scale: Rc<Cell<bool>>,
    updating_input_scale: Rc<Cell<bool>>,
    outputs: Controller<DeviceList<Command>>,
    inputs: Controller<DeviceList<Command>>,
    streams: Controller<DeviceList<Command>>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub max_volume: u32,
    pub show_streams: bool,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    Reconfigure { max_volume: u32, show_streams: bool },
    ToggleStreams,
    Command(Command),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Command(Command),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "audio-popover",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[name = "hero"]
                    #[template]
                    HeroView {},

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,

                            #[name = "output_mute"]
                            gtk::Button {
                                add_css_class: "flat",
                                set_icon_name: "audio-volume-high-symbolic",
                                connect_clicked => PopoverInput::Command(Command::ToggleOutputMute),
                            },

                            #[name = "output_scale"]
                            gtk::Scale {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_draw_value: false,
                                set_hexpand: true,
                                set_range: (0.0, init.max_volume as f64),
                                set_increments: (1.0, 5.0),
                            },
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,

                            #[name = "input_mute"]
                            gtk::Button {
                                add_css_class: "flat",
                                set_icon_name: "audio-input-microphone-symbolic",
                                connect_clicked => PopoverInput::Command(Command::ToggleInputMute),
                            },

                            #[name = "input_scale"]
                            gtk::Scale {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_draw_value: false,
                                set_hexpand: true,
                                set_range: (0.0, init.max_volume as f64),
                                set_increments: (1.0, 5.0),
                            },
                        },
                    },

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[local_ref]
                    outputs_widget -> gtk::Box {},

                    #[local_ref]
                    inputs_widget -> gtk::Box {},

                    #[name = "streams_section"]
                    #[template]
                    CollapsibleSectionView {
                        #[template_child]
                        title {
                            set_label: "Apps",
                        },

                        #[template_child]
                        button {
                            connect_clicked => PopoverInput::ToggleStreams,
                        },

                        #[template_child]
                        content {
                            #[local_ref]
                            streams_widget -> gtk::Box {},
                        },
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let outputs = DeviceList::builder()
            .launch(DeviceListInit {
                header: Some("Output devices".into()),
                items: Vec::new(),
            })
            .forward(sender.input_sender(), PopoverInput::Command);
        let outputs_widget = outputs.widget().clone();

        let inputs = DeviceList::builder()
            .launch(DeviceListInit {
                header: Some("Input devices".into()),
                items: Vec::new(),
            })
            .forward(sender.input_sender(), PopoverInput::Command);
        let inputs_widget = inputs.widget().clone();

        let streams = DeviceList::builder()
            .launch(DeviceListInit {
                header: None,
                items: Vec::new(),
            })
            .forward(sender.input_sender(), PopoverInput::Command);
        let streams_widget = streams.widget().clone();

        let updating_output_scale = Rc::new(Cell::new(false));
        let updating_input_scale = Rc::new(Cell::new(false));
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        if init.max_volume > 100 {
            widgets
                .output_scale
                .add_mark(100.0, gtk::PositionType::Bottom, None);
            widgets
                .input_scale
                .add_mark(100.0, gtk::PositionType::Bottom, None);
        }

        connect_throttled_scale(
            &widgets.output_scale,
            updating_output_scale.clone(),
            sender.clone(),
            Command::SetOutputVolume,
        );
        connect_throttled_scale(
            &widgets.input_scale,
            updating_input_scale.clone(),
            sender.clone(),
            Command::SetInputVolume,
        );

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            state: State::default(),
            max_volume: init.max_volume,
            show_streams: init.show_streams,
            streams_expanded: false,
            updating_output_scale,
            updating_input_scale,
            outputs,
            inputs,
            streams,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => {
                self.animation.toggle();
            }
            PopoverInput::UpdateState(state) => {
                self.outputs
                    .emit(DeviceListInput::Update(output_items(&state.outputs)));
                self.inputs
                    .emit(DeviceListInput::Update(input_items(&state.inputs)));
                self.streams
                    .emit(DeviceListInput::Update(stream_items(&state.streams)));
                self.state = state;
            }
            PopoverInput::Reconfigure {
                max_volume,
                show_streams,
            } => {
                self.max_volume = max_volume;
                self.show_streams = show_streams;
            }
            PopoverInput::ToggleStreams => {
                self.streams_expanded = !self.streams_expanded;
            }
            PopoverInput::Command(command) => {
                let _ = sender.output(PopoverOutput::Command(command));
            }
        }
    }

    fn post_view() {
        let output = model.state.default_output();
        let input = model.state.default_input();

        hero.icon.set_icon_name(Some(
            output
                .map(|device| volume_icon(device.volume, device.muted))
                .unwrap_or("audio-volume-muted-symbolic"),
        ));
        hero.title.set_label("Audio");
        hero.subtitle.set_label(&hero_subtitle(&model.state));

        output_mute.set_icon_name(
            output
                .map(|device| volume_icon(device.volume, device.muted))
                .unwrap_or("audio-volume-high-symbolic"),
        );
        output_mute.set_sensitive(output.is_some());
        input_mute.set_icon_name(input_icon_name(input));
        input_mute.set_sensitive(input.is_some());

        output_scale.set_range(0.0, model.max_volume as f64);
        output_scale.set_sensitive(output.is_some());
        input_scale.set_range(0.0, model.max_volume as f64);
        input_scale.set_sensitive(input.is_some());

        if let Some(device) = output {
            model.updating_output_scale.set(true);
            output_scale.set_value(device.volume as f64);
            model.updating_output_scale.set(false);
        }

        if let Some(device) = input {
            model.updating_input_scale.set(true);
            input_scale.set_value(device.volume as f64);
            model.updating_input_scale.set(false);
        }

        streams_section.set_visible(model.show_streams);
        streams_section.content.set_visible(model.streams_expanded);
        streams_section
            .chevron
            .set_icon_name(Some(if model.streams_expanded {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            }));
        streams_section
            .title
            .set_label(&format!("Apps ({})", model.state.streams.len()));
    }
}

fn connect_throttled_scale(
    scale: &gtk::Scale,
    updating: Rc<Cell<bool>>,
    sender: ComponentSender<Popover>,
    make_command: fn(u32) -> Command,
) {
    let last_sent = Rc::new(Cell::new(Instant::now()));
    let pending = Rc::new(Cell::new(false));

    scale.connect_change_value(move |scale, _, value| {
        if updating.get() {
            return glib::Propagation::Stop;
        }

        let now = Instant::now();
        if now.duration_since(last_sent.get()) >= Duration::from_millis(100) {
            last_sent.set(now);
            pending.set(false);
            sender.input(PopoverInput::Command(make_command(value as u32)));
        } else if !pending.get() {
            pending.set(true);
            let last_sent = last_sent.clone();
            let pending = pending.clone();
            let sender = sender.clone();
            let scale = scale.clone();
            glib::timeout_add_local_once(Duration::from_millis(100), move || {
                if pending.get() {
                    pending.set(false);
                    last_sent.set(Instant::now());
                    sender.input(PopoverInput::Command(make_command(scale.value() as u32)));
                }
            });
        }

        glib::Propagation::Proceed
    });
}

fn output_items(devices: &[AudioDevice]) -> Vec<DeviceListItem<Command>> {
    devices
        .iter()
        .map(|device| DeviceListItem {
            id: device.name.clone(),
            icon: device.icon_name.clone(),
            label: device.description.clone(),
            status: if device.muted {
                "Muted".into()
            } else {
                format!("{}%", device.volume)
            },
            busy: false,
            tooltip: Some(device_tooltip(device)),
            active: device.is_default,
            visible: true,
            command: Some(Command::SetDefaultOutput(device.name.clone())),
        })
        .collect()
}

fn input_items(devices: &[AudioDevice]) -> Vec<DeviceListItem<Command>> {
    devices
        .iter()
        .map(|device| DeviceListItem {
            id: device.name.clone(),
            icon: device.icon_name.clone(),
            label: device.description.clone(),
            status: if device.muted {
                "Muted".into()
            } else {
                format!("{}%", device.volume)
            },
            busy: false,
            tooltip: Some(device_tooltip(device)),
            active: device.is_default,
            visible: true,
            command: Some(Command::SetDefaultInput(device.name.clone())),
        })
        .collect()
}

fn stream_items(streams: &[AudioStream]) -> Vec<DeviceListItem<Command>> {
    streams
        .iter()
        .map(|stream| DeviceListItem {
            id: stream.index.to_string(),
            icon: stream.app_icon.clone(),
            label: stream.app_name.clone(),
            status: if stream.muted {
                "Muted".into()
            } else {
                format!("{}%", stream.volume)
            },
            busy: false,
            tooltip: Some(format!("{}%", stream.volume)),
            active: false,
            visible: true,
            command: Some(Command::ToggleStreamMute(stream.index)),
        })
        .collect()
}

fn hero_subtitle(state: &State) -> String {
    if !state.available {
        return "Unavailable".into();
    }

    state
        .default_output()
        .map(|device| {
            if device.muted {
                format!("{} muted", device.description)
            } else {
                device.description.clone()
            }
        })
        .unwrap_or_else(|| "No output device".into())
}

fn input_icon_name(device: Option<&AudioDevice>) -> &'static str {
    match device {
        Some(device) if device.muted => "microphone-sensitivity-muted-symbolic",
        _ => "audio-input-microphone-symbolic",
    }
}

fn device_tooltip(device: &AudioDevice) -> String {
    if device.muted {
        format!("{} muted", device.description)
    } else {
        format!("{} {}%", device.description, device.volume)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_items_map_devices_to_device_list_commands() {
        let items = output_items(&[AudioDevice {
            index: 1,
            name: "sink".into(),
            description: "Speakers".into(),
            volume: 70,
            muted: false,
            is_default: true,
            icon_name: "audio-speakers-symbolic".into(),
        }]);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "Speakers");
        assert_eq!(items[0].status, "70%");
        assert_eq!(
            items[0].command,
            Some(Command::SetDefaultOutput("sink".into()))
        );
    }
}
