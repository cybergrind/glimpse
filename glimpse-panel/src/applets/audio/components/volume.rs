use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use glimpse::providers::audio::{AudioProvider, DeviceList, volume_icon};
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::{self, glib, prelude::*}};

pub struct VolumeSection {
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
    ToggleOutputMute,
    ToggleInputMute,
    SetOutputVolume(u32),
    SetInputVolume(u32),
}

impl SimpleComponent for VolumeSection {
    type Init = VolumeSectionInit;
    type Input = VolumeSectionInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let max_vol = init.max_volume;

        let output_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        output_row.add_css_class("volume-row");
        let output_mute_btn = gtk::Button::from_icon_name("audio-volume-high-symbolic");
        output_mute_btn.add_css_class("flat");
        output_mute_btn.add_css_class("mute-btn");
        output_mute_btn.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(VolumeSectionInput::ToggleOutputMute)
        });
        output_row.append(&output_mute_btn);
        let output_scale = build_throttled_scale(max_vol, sender.clone(), VolumeSectionInput::SetOutputVolume);
        output_row.append(&output_scale);
        root.append(&output_row);

        let input_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        input_row.add_css_class("volume-row");
        let input_mute_btn = gtk::Button::from_icon_name("audio-input-microphone-symbolic");
        input_mute_btn.add_css_class("flat");
        input_mute_btn.add_css_class("mute-btn");
        input_mute_btn.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(VolumeSectionInput::ToggleInputMute)
        });
        input_row.append(&input_mute_btn);
        let input_scale = build_throttled_scale(max_vol, sender, VolumeSectionInput::SetInputVolume);
        input_row.append(&input_scale);
        root.append(&input_row);

        let model = VolumeSection { output_scale, output_mute_btn, input_scale, input_mute_btn };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            VolumeSectionInput::UpdateOutputs(outputs) => {
                if let Some(d) = outputs.default_device() {
                    if !is_dragging(&self.output_scale) {
                        self.output_scale.set_value(d.volume as f64);
                    }
                    self.output_mute_btn.set_icon_name(volume_icon(d.volume, d.muted));
                    self.output_mute_btn
                        .set_tooltip_text(Some(&format!("{} — {}%", d.description, d.volume)));
                }
            }
            VolumeSectionInput::UpdateInputs(inputs) => {
                if let Some(d) = inputs.default_device() {
                    if !is_dragging(&self.input_scale) {
                        self.input_scale.set_value(d.volume as f64);
                    }
                    self.input_mute_btn.set_icon_name(if d.muted {
                        "microphone-sensitivity-muted-symbolic"
                    } else {
                        "audio-input-microphone-symbolic"
                    });
                    self.input_mute_btn
                        .set_tooltip_text(Some(&format!("{} — {}%", d.description, d.volume)));
                }
            }
            VolumeSectionInput::ToggleOutputMute => {
                glib::spawn_future_local(async move {
                    if let Err(e) = AudioProvider::new().toggle_mute("@DEFAULT_SINK@").await {
                        tracing::warn!("audio: toggle output mute: {e}");
                    }
                });
            }
            VolumeSectionInput::ToggleInputMute => {
                glib::spawn_future_local(async move {
                    if let Err(e) = AudioProvider::new().toggle_mute("@DEFAULT_SOURCE@").await {
                        tracing::warn!("audio: toggle input mute: {e}");
                    }
                });
            }
            VolumeSectionInput::SetOutputVolume(vol) => {
                glib::spawn_future_local(async move {
                    if let Err(e) = AudioProvider::new().set_volume("@DEFAULT_SINK@", vol).await {
                        tracing::warn!("audio: set output volume: {e}");
                    }
                });
            }
            VolumeSectionInput::SetInputVolume(vol) => {
                glib::spawn_future_local(async move {
                    if let Err(e) = AudioProvider::new().set_volume("@DEFAULT_SOURCE@", vol).await {
                        tracing::warn!("audio: set input volume: {e}");
                    }
                });
            }
        }
    }
}

fn is_dragging(scale: &gtk::Scale) -> bool {
    scale.state_flags().contains(gtk::StateFlags::ACTIVE)
}

fn build_throttled_scale(
    max_vol: f64,
    sender: ComponentSender<VolumeSection>,
    make_msg: fn(u32) -> VolumeSectionInput,
) -> gtk::Scale {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, max_vol, 1.0);
    scale.set_hexpand(true);
    if max_vol > 100.0 {
        scale.add_mark(100.0, gtk::PositionType::Bottom, None);
    }

    let last_sent = Rc::new(Cell::new(Instant::now()));
    let pending = Rc::new(Cell::new(false));

    scale.connect_change_value(move |scale, _, val| {
        let now = Instant::now();
        if now.duration_since(last_sent.get()).as_millis() >= 100 {
            last_sent.set(now);
            pending.set(false);
            sender.input(make_msg(val as u32));
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
                    sender.input(make_msg(scale.value() as u32));
                }
            });
        }
        glib::Propagation::Proceed
    });

    scale
}
