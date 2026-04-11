use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use glimpse::providers::audio::{volume_icon, AudioDevice, DeviceList};
use relm4::{
    gtk::{self, glib, prelude::*},
    ComponentParts, ComponentSender, SimpleComponent,
};

pub struct VolumeSection {
    output_device: Option<AudioDevice>,
    input_device: Option<AudioDevice>,
    output_scale: gtk::Scale,
    output_mute_btn: gtk::Button,
    input_scale: gtk::Scale,
    input_mute_btn: gtk::Button,
}

pub struct VolumeSectionInit {
    pub max_volume: f64,
}

#[derive(Debug)]
pub enum VolumeSectionInput {
    UpdateOutputs(DeviceList),
    UpdateInputs(DeviceList),
}

#[derive(Debug, Clone, PartialEq)]
pub enum VolumeSectionOutput {
    ToggleOutputMute,
    ToggleInputMute,
    SetOutputVolume(u32),
    SetInputVolume(u32),
}

#[relm4::component(pub)]
impl SimpleComponent for VolumeSection {
    type Init = VolumeSectionInit;
    type Input = VolumeSectionInput;
    type Output = VolumeSectionOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,

            gtk::Box {
                set_spacing: 8,
                add_css_class: "volume-row",

                #[name(output_mute_btn)]
                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "mute-btn",
                    set_icon_name: "audio-volume-high-symbolic",
                    connect_clicked[sender] => move |_| {
                        let _ = sender.output(VolumeSectionOutput::ToggleOutputMute);
                    },
                },

                #[name(output_scale)]
                gtk::Scale {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_draw_value: false,
                    set_hexpand: true,
                    set_range: (0.0, init.max_volume),
                    set_increments: (1.0, 5.0),
                },
            },

            gtk::Box {
                set_spacing: 8,
                add_css_class: "volume-row",

                #[name(input_mute_btn)]
                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "mute-btn",
                    set_icon_name: "audio-input-microphone-symbolic",
                    connect_clicked[sender] => move |_| {
                        let _ = sender.output(VolumeSectionOutput::ToggleInputMute);
                    },
                },

                #[name(input_scale)]
                gtk::Scale {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_draw_value: false,
                    set_hexpand: true,
                    set_range: (0.0, init.max_volume),
                    set_increments: (1.0, 5.0),
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();

        if init.max_volume > 100.0 {
            widgets
                .output_scale
                .add_mark(100.0, gtk::PositionType::Bottom, None);
            widgets
                .input_scale
                .add_mark(100.0, gtk::PositionType::Bottom, None);
        }

        connect_throttled_scale(
            &widgets.output_scale,
            sender.clone(),
            VolumeSectionOutput::SetOutputVolume,
        );
        connect_throttled_scale(
            &widgets.input_scale,
            sender.clone(),
            VolumeSectionOutput::SetInputVolume,
        );

        let model = VolumeSection {
            output_device: None,
            input_device: None,
            output_scale: widgets.output_scale.clone(),
            output_mute_btn: widgets.output_mute_btn.clone(),
            input_scale: widgets.input_scale.clone(),
            input_mute_btn: widgets.input_mute_btn.clone(),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            VolumeSectionInput::UpdateOutputs(outputs) => {
                self.output_device = outputs.default_device().cloned();
                if let Some(device) = self.output_device.as_ref() {
                    if !is_dragging(&self.output_scale) {
                        self.output_scale.set_value(device.volume as f64);
                    }
                    self.output_mute_btn
                        .set_icon_name(output_icon_name(Some(device)));
                    self.output_mute_btn
                        .set_tooltip_text(output_tooltip(Some(device)).as_deref());
                }
            }
            VolumeSectionInput::UpdateInputs(inputs) => {
                self.input_device = inputs.default_device().cloned();
                if let Some(device) = self.input_device.as_ref() {
                    if !is_dragging(&self.input_scale) {
                        self.input_scale.set_value(device.volume as f64);
                    }
                    self.input_mute_btn
                        .set_icon_name(input_icon_name(Some(device)));
                    self.input_mute_btn
                        .set_tooltip_text(input_tooltip(Some(device)).as_deref());
                }
            }
        }
    }
}

fn is_dragging(scale: &gtk::Scale) -> bool {
    scale.state_flags().contains(gtk::StateFlags::ACTIVE)
}

fn connect_throttled_scale(
    scale: &gtk::Scale,
    sender: ComponentSender<VolumeSection>,
    make_output: fn(u32) -> VolumeSectionOutput,
) {
    let last_sent = Rc::new(Cell::new(Instant::now()));
    let pending = Rc::new(Cell::new(false));

    scale.connect_change_value(move |scale, _, value| {
        let now = Instant::now();
        if now.duration_since(last_sent.get()).as_millis() >= 100 {
            last_sent.set(now);
            pending.set(false);
            let _ = sender.output(make_output(value as u32));
        } else if !pending.get() {
            pending.set(true);
            let last_sent = last_sent.clone();
            let pending = pending.clone();
            let scale = scale.clone();
            let sender = sender.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(100), move || {
                if pending.get() {
                    pending.set(false);
                    last_sent.set(Instant::now());
                    let _ = sender.output(make_output(scale.value() as u32));
                }
            });
        }

        glib::Propagation::Proceed
    });
}

fn output_icon_name(device: Option<&AudioDevice>) -> &'static str {
    device
        .map(|device| volume_icon(device.volume, device.muted))
        .unwrap_or("audio-volume-high-symbolic")
}

fn input_icon_name(device: Option<&AudioDevice>) -> &'static str {
    match device {
        Some(device) if device.muted => "microphone-sensitivity-muted-symbolic",
        _ => "audio-input-microphone-symbolic",
    }
}

fn output_tooltip(device: Option<&AudioDevice>) -> Option<String> {
    device.map(|device| format!("{} — {}%", device.description, device.volume))
}

fn input_tooltip(device: Option<&AudioDevice>) -> Option<String> {
    device.map(|device| format!("{} — {}%", device.description, device.volume))
}
