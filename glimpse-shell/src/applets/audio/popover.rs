#![allow(unused_assignments)]

use std::cell::{Cell, RefCell};
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

const VOLUME_ECHO_GRACE: Duration = Duration::from_secs(2);
const VOLUME_COMMAND_INTERVAL: Duration = Duration::from_millis(50);

pub struct Popover {
    animation: AnimatedPopover,
    state: State,
    max_volume: u32,
    show_streams: bool,
    outputs_expanded: bool,
    inputs_expanded: bool,
    streams_expanded: bool,
    pending_output_volume: Rc<RefCell<Option<PendingVolume>>>,
    pending_input_volume: Rc<RefCell<Option<PendingVolume>>>,
    updating_output_scale: Rc<Cell<bool>>,
    updating_input_scale: Rc<Cell<bool>>,
    outputs: Controller<DeviceList<Command>>,
    inputs: Controller<DeviceList<Command>>,
    streams: Controller<DeviceList<Command>>,
}

#[derive(Debug, Clone)]
struct PendingVolume {
    value: u32,
    changed_at: Instant,
}

impl PendingVolume {
    fn new(value: u32) -> Self {
        Self {
            value,
            changed_at: Instant::now(),
        }
    }
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
    ToggleOutputs,
    ToggleInputs,
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

                    #[name = "outputs_section"]
                    #[template]
                    CollapsibleSectionView {
                        add_css_class: "audio-device-section",

                        #[template_child]
                        title {
                            set_label: "Output devices",
                        },

                        #[template_child]
                        button {
                            connect_clicked => PopoverInput::ToggleOutputs,
                        },

                        #[template_child]
                        content {
                            #[local_ref]
                            outputs_widget -> gtk::Box {},
                        },
                    },

                    #[name = "inputs_section"]
                    #[template]
                    CollapsibleSectionView {
                        add_css_class: "audio-device-section",

                        #[template_child]
                        title {
                            set_label: "Input devices",
                        },

                        #[template_child]
                        button {
                            connect_clicked => PopoverInput::ToggleInputs,
                        },

                        #[template_child]
                        content {
                            #[local_ref]
                            inputs_widget -> gtk::Box {},
                        },
                    },

                    #[name = "streams_section"]
                    #[template]
                    CollapsibleSectionView {
                        add_css_class: "audio-device-section",

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
                header: None,
                items: Vec::new(),
            })
            .forward(sender.input_sender(), PopoverInput::Command);
        let outputs_widget = outputs.widget().clone();

        let inputs = DeviceList::builder()
            .launch(DeviceListInit {
                header: None,
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

        let pending_output_volume = Rc::new(RefCell::new(None));
        let pending_input_volume = Rc::new(RefCell::new(None));

        connect_throttled_scale(
            &widgets.output_scale,
            updating_output_scale.clone(),
            pending_output_volume.clone(),
            sender.clone(),
            Command::SetOutputVolume,
        );
        connect_throttled_scale(
            &widgets.input_scale,
            updating_input_scale.clone(),
            pending_input_volume.clone(),
            sender.clone(),
            Command::SetInputVolume,
        );

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            state: State::default(),
            max_volume: init.max_volume,
            show_streams: init.show_streams,
            outputs_expanded: false,
            inputs_expanded: false,
            streams_expanded: false,
            pending_output_volume,
            pending_input_volume,
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
            PopoverInput::ToggleOutputs => {
                self.outputs_expanded = !self.outputs_expanded;
            }
            PopoverInput::ToggleInputs => {
                self.inputs_expanded = !self.inputs_expanded;
            }
            PopoverInput::ToggleStreams => {
                self.streams_expanded = !self.streams_expanded;
            }
            PopoverInput::Command(command) => {
                match &command {
                    Command::SetOutputVolume(volume) => {
                        self.pending_output_volume
                            .replace(Some(PendingVolume::new(*volume)));
                    }
                    Command::SetInputVolume(volume) => {
                        self.pending_input_volume
                            .replace(Some(PendingVolume::new(*volume)));
                    }
                    _ => {}
                }
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

        let now = Instant::now();
        if let Some(device) = output
            && !scale_is_dragging(&output_scale)
        {
            let should_apply = {
                let mut pending = model.pending_output_volume.borrow_mut();
                should_apply_service_volume(&mut pending, device.volume, now)
            };
            if should_apply {
                model.updating_output_scale.set(true);
                output_scale.set_value(device.volume as f64);
                model.updating_output_scale.set(false);
            }
        }

        if let Some(device) = input
            && !scale_is_dragging(&input_scale)
        {
            let should_apply = {
                let mut pending = model.pending_input_volume.borrow_mut();
                should_apply_service_volume(&mut pending, device.volume, now)
            };
            if should_apply {
                model.updating_input_scale.set(true);
                input_scale.set_value(device.volume as f64);
                model.updating_input_scale.set(false);
            }
        }

        sync_collapsible_section(&outputs_section, model.outputs_expanded);
        sync_collapsible_section(&inputs_section, model.inputs_expanded);
        streams_section.set_visible(model.show_streams);
        sync_collapsible_section(&streams_section, model.streams_expanded);
        streams_section
            .title
            .set_label(&format!("Apps ({})", model.state.streams.len()));
    }
}

fn scale_is_dragging(scale: &gtk::Scale) -> bool {
    scale.state_flags().contains(gtk::StateFlags::ACTIVE)
}

fn sync_collapsible_section(section: &CollapsibleSectionView, expanded: bool) {
    section.content.set_visible(expanded);
    section.chevron.set_icon_name(Some(if expanded {
        "pan-down-symbolic"
    } else {
        "pan-end-symbolic"
    }));
}

fn should_apply_service_volume(
    pending: &mut Option<PendingVolume>,
    service_volume: u32,
    now: Instant,
) -> bool {
    let Some(value) = pending else {
        return true;
    };

    if value.value == service_volume {
        *pending = None;
        return true;
    }

    if now.duration_since(value.changed_at) < VOLUME_ECHO_GRACE {
        return false;
    }

    *pending = None;
    true
}

fn connect_throttled_scale(
    scale: &gtk::Scale,
    updating: Rc<Cell<bool>>,
    pending_volume: Rc<RefCell<Option<PendingVolume>>>,
    sender: ComponentSender<Popover>,
    make_command: fn(u32) -> Command,
) {
    let last_sent = Rc::new(Cell::new(Instant::now() - VOLUME_COMMAND_INTERVAL));
    let pending = Rc::new(Cell::new(false));
    let pending_value = Rc::new(Cell::new(0));

    scale.connect_change_value(move |_, _, value| {
        if updating.get() {
            return glib::Propagation::Stop;
        }

        let volume = volume_from_scale_value(value);
        pending_value.set(volume);
        pending_volume
            .borrow_mut()
            .replace(PendingVolume::new(volume));

        let now = Instant::now();
        if now.duration_since(last_sent.get()) >= VOLUME_COMMAND_INTERVAL {
            last_sent.set(now);
            pending.set(false);
            sender.input(PopoverInput::Command(make_command(volume)));
        } else if !pending.get() {
            pending.set(true);
            let last_sent = last_sent.clone();
            let pending = pending.clone();
            let pending_value = pending_value.clone();
            let sender = sender.clone();
            let delay = VOLUME_COMMAND_INTERVAL.saturating_sub(now.duration_since(last_sent.get()));
            glib::timeout_add_local_once(delay, move || {
                if pending.get() {
                    pending.set(false);
                    last_sent.set(Instant::now());
                    sender.input(PopoverInput::Command(make_command(pending_value.get())));
                }
            });
        }

        glib::Propagation::Proceed
    });
}

fn volume_from_scale_value(value: f64) -> u32 {
    value.round().clamp(0.0, u32::MAX as f64) as u32
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

    #[test]
    fn pending_volume_ignores_recent_stale_service_values() {
        let now = Instant::now();
        let mut pending = Some(PendingVolume {
            value: 80,
            changed_at: now,
        });

        assert!(!should_apply_service_volume(&mut pending, 40, now));
        assert!(pending.is_some());
    }

    #[test]
    fn pending_volume_clears_when_service_catches_up() {
        let now = Instant::now();
        let mut pending = Some(PendingVolume {
            value: 80,
            changed_at: now,
        });

        assert!(should_apply_service_volume(&mut pending, 80, now));
        assert!(pending.is_none());
    }

    #[test]
    fn pending_volume_expires_if_service_never_catches_up() {
        let now = Instant::now();
        let mut pending = Some(PendingVolume {
            value: 80,
            changed_at: now - VOLUME_ECHO_GRACE - Duration::from_millis(1),
        });

        assert!(should_apply_service_volume(&mut pending, 40, now));
        assert!(pending.is_none());
    }

    #[test]
    fn scale_values_are_normalized_before_commands() {
        assert_eq!(volume_from_scale_value(-1.0), 0);
        assert_eq!(volume_from_scale_value(40.4), 40);
        assert_eq!(volume_from_scale_value(40.6), 41);
    }
}
