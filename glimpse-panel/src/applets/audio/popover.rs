use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};
use serde::Deserialize;

use super::config::AudioConfig;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AudioOutput {
    pub name: String,
    pub description: String,
    pub volume: u32,
    pub muted: bool,
    pub is_default: bool,
    pub icon_name: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AudioInput {
    pub name: String,
    pub description: String,
    pub volume: u32,
    pub muted: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AudioStream {
    pub index: u64,
    pub app_name: String,
    pub app_icon: String,
    pub volume: u32,
    pub muted: bool,
}

pub struct Popover {
    popover: gtk::Popover,
    config: AudioConfig,
    client: Arc<Client>,
    output_scale: gtk::Scale,
    output_vol_label: gtk::Label,
    output_mute_btn: gtk::Button,
    output_header_icon: gtk::Image,
    output_header_label: gtk::Label,
    output_header_chevron: gtk::Label,
    output_devices_box: gtk::Box,
    input_scale: gtk::Scale,
    input_vol_label: gtk::Label,
    input_mute_btn: gtk::Button,
    input_header_label: gtk::Label,
    input_header_chevron: gtk::Label,
    input_devices_box: gtk::Box,
    streams_btn_label: gtk::Label,
    streams_box: gtk::Box,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub client: Arc<Client>,
    pub config: AudioConfig,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateOutputs(Vec<AudioOutput>),
    UpdateInputs(Vec<AudioInput>),
    UpdateStreams(Vec<AudioStream>),
}

fn spawn_call(client: &Arc<Client>, method: &'static str, params: serde_json::Value) {
    let c = client.clone();
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move { let _ = c.call(method, params).await; });
    } else {
        tracing::warn!("audio: no tokio runtime for {method}");
    }
}

impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = ();
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root { gtk::Popover::new() }

    fn init(
        init: Self::Init, root: Self::Root, _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.set_has_arrow(false);
        root.add_css_class("audio-popover");

        let max_vol = init.config.max_volume as f64;
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        // Output volume.
        let (output_scale, output_vol_label, output_mute_btn) =
            build_volume_row(&vbox, max_vol, &init.client, None);

        // Output device selector.
        let (output_header_icon, output_header_label, output_header_chevron, output_devices_box) =
            build_device_section(&vbox, "audio-speakers-symbolic");

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // Input volume.
        let (input_scale, input_vol_label, input_mute_btn) =
            build_volume_row(&vbox, max_vol, &init.client, Some("@DEFAULT_SOURCE@"));

        // Input device selector.
        let (_, input_header_label, input_header_chevron, input_devices_box) =
            build_device_section(&vbox, "audio-input-microphone-symbolic");

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // Streams.
        let (streams_btn_label, streams_box) = build_collapsible(&vbox, "Apps");

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // Settings.
        if !init.config.settings_command.is_empty() {
            let cmd = init.config.settings_command.clone();
            let lbl = gtk::Label::new(Some("Audio Settings"));
            lbl.set_halign(gtk::Align::Start);
            let btn = gtk::Button::new();
            btn.set_child(Some(&lbl));
            btn.add_css_class("flat");
            btn.connect_clicked(move |_| {
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if let Some((&prog, args)) = parts.split_first() {
                    let _ = std::process::Command::new(prog).args(args).spawn();
                }
            });
            vbox.append(&btn);
        }

        root.set_child(Some(&vbox));

        // Mute toggles.
        let c = init.client.clone();
        output_mute_btn.connect_clicked(move |_| {
            spawn_call(&c, "audio.set_mute", serde_json::json!({}));
        });
        let c = init.client.clone();
        input_mute_btn.connect_clicked(move |_| {
            spawn_call(&c, "audio.set_mute", serde_json::json!({"target": "@DEFAULT_SOURCE@"}));
        });

        let model = Popover {
            popover: root.clone(), config: init.config, client: init.client,
            output_scale, output_vol_label, output_mute_btn,
            output_header_icon, output_header_label, output_header_chevron, output_devices_box,
            input_scale, input_vol_label, input_mute_btn,
            input_header_label, input_header_chevron, input_devices_box,
            streams_btn_label, streams_box,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            PopoverInput::Toggle => {
                if self.popover.is_visible() { self.popover.popdown(); }
                else { self.popover.popup(); }
            }
            PopoverInput::UpdateOutputs(outputs) => {
                if let Some(d) = outputs.iter().find(|o| o.is_default) {
                    if !is_being_dragged(&self.output_scale) {
                        self.output_scale.set_value(d.volume as f64);
                    }
                    self.output_vol_label.set_label(&format!("{}%", d.volume));
                    self.output_mute_btn.set_icon_name(
                        if d.muted { "audio-volume-muted-symbolic" }
                        else { "audio-volume-high-symbolic" }
                    );
                    self.output_header_label.set_label(&d.description);
                    self.output_header_label.set_tooltip_text(Some(&format!("{} — {}%", d.description, d.volume)));
                    if !d.icon_name.is_empty() {
                        self.output_header_icon.set_icon_name(Some(&d.icon_name));
                    }
                }
                rebuild_device_list(
                    &self.output_devices_box,
                    &outputs.iter().map(|o| DeviceInfo {
                        name: &o.name, description: &o.description,
                        icon_name: &o.icon_name, is_default: o.is_default,
                    }).collect::<Vec<_>>(),
                    &self.client, "audio.set_default_output",
                    &self.output_devices_box, &self.output_header_chevron,
                );
            }
            PopoverInput::UpdateInputs(inputs) => {
                if let Some(d) = inputs.iter().find(|i| i.is_default) {
                    if !is_being_dragged(&self.input_scale) {
                        self.input_scale.set_value(d.volume as f64);
                    }
                    self.input_vol_label.set_label(&format!("{}%", d.volume));
                    self.input_mute_btn.set_icon_name(
                        if d.muted { "microphone-sensitivity-muted-symbolic" }
                        else { "audio-input-microphone-symbolic" }
                    );
                    self.input_header_label.set_label(&d.description);
                    self.input_header_label.set_tooltip_text(Some(&format!("{} — {}%", d.description, d.volume)));
                }
                rebuild_device_list(
                    &self.input_devices_box,
                    &inputs.iter().map(|i| DeviceInfo {
                        name: &i.name, description: &i.description,
                        icon_name: "audio-input-microphone-symbolic", is_default: i.is_default,
                    }).collect::<Vec<_>>(),
                    &self.client, "audio.set_default_input",
                    &self.input_devices_box, &self.input_header_chevron,
                );
            }
            PopoverInput::UpdateStreams(streams) => {
                let count = streams.len();
                self.streams_btn_label.set_label(&format!("Apps ({count})"));

                clear_box(&self.streams_box);
                let max_vol = self.config.max_volume as f64;
                for stream in &streams {
                    build_stream_row(&self.streams_box, stream, max_vol, &self.client);
                }
            }
        }
    }
}

fn is_being_dragged(scale: &gtk::Scale) -> bool {
    scale.state_flags().contains(gtk::StateFlags::ACTIVE)
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn build_volume_row(
    parent: &gtk::Box, max_vol: f64, client: &Arc<Client>, target: Option<&str>,
) -> (gtk::Scale, gtk::Label, gtk::Button) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("volume-row");

    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, max_vol, 1.0);
    scale.set_hexpand(true);
    if max_vol > 100.0 { scale.add_mark(100.0, gtk::PositionType::Bottom, None); }

    let vol_label = gtk::Label::new(Some("0%"));
    vol_label.set_width_chars(4);
    vol_label.add_css_class("audio-label");

    let vol_c = vol_label.clone();
    scale.connect_value_changed(move |s| {
        vol_c.set_label(&format!("{}%", s.value() as u32));
    });

    let c = client.clone();
    let t = target.map(|s| s.to_owned());
    scale.connect_change_value(move |_, _, val| {
        let mut params = serde_json::json!({"volume": val as u32});
        if let Some(ref t) = t { params["target"] = serde_json::json!(t); }
        spawn_call(&c, "audio.set_volume", params);
        glib::Propagation::Proceed
    });

    let mute_btn = gtk::Button::from_icon_name("audio-volume-high-symbolic");
    mute_btn.add_css_class("flat");

    row.append(&scale);
    row.append(&vol_label);
    row.append(&mute_btn);
    parent.append(&row);
    (scale, vol_label, mute_btn)
}

fn build_device_section(
    parent: &gtk::Box, default_icon: &str,
) -> (gtk::Image, gtk::Label, gtk::Label, gtk::Box) {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("device-header");

    let icon = gtk::Image::from_icon_name(default_icon);
    icon.set_pixel_size(16);
    header.append(&icon);

    let label = gtk::Label::new(None);
    label.set_hexpand(true);
    label.set_halign(gtk::Align::Start);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_max_width_chars(30);
    header.append(&label);

    let chevron = gtk::Label::new(Some("›"));
    chevron.add_css_class("chevron");
    header.append(&chevron);

    let btn = gtk::Button::new();
    btn.set_child(Some(&header));
    btn.add_css_class("flat");
    parent.append(&btn);

    let devices_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    devices_box.set_visible(false);
    devices_box.add_css_class("device-list");
    parent.append(&devices_box);

    let box_ref = devices_box.clone();
    let chevron_ref = chevron.clone();
    btn.connect_clicked(move |_| {
        let show = !box_ref.is_visible();
        box_ref.set_visible(show);
        chevron_ref.set_label(if show { "⌄" } else { "›" });
    });

    (icon, label, chevron, devices_box)
}

fn build_collapsible(parent: &gtk::Box, label: &str) -> (gtk::Label, gtk::Box) {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);

    let name_label = gtk::Label::new(Some(label));
    name_label.set_hexpand(true);
    name_label.set_halign(gtk::Align::Start);
    header.append(&name_label);

    let chevron = gtk::Label::new(Some("›"));
    chevron.add_css_class("chevron");
    header.append(&chevron);

    let btn = gtk::Button::new();
    btn.set_child(Some(&header));
    btn.add_css_class("flat");
    parent.append(&btn);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
    content.set_visible(false);
    content.add_css_class("streams-list");
    parent.append(&content);

    let content_ref = content.clone();
    let chevron_ref = chevron.clone();
    btn.connect_clicked(move |_| {
        let show = !content_ref.is_visible();
        content_ref.set_visible(show);
        chevron_ref.set_label(if show { "⌄" } else { "›" });
    });

    (name_label, content)
}

struct DeviceInfo<'a> {
    name: &'a str,
    description: &'a str,
    icon_name: &'a str,
    is_default: bool,
}

fn rebuild_device_list(
    container: &gtk::Box, devices: &[DeviceInfo], client: &Arc<Client>,
    method: &'static str, devices_box: &gtk::Box, chevron: &gtk::Label,
) {
    clear_box(container);
    for dev in devices {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let icon = gtk::Image::from_icon_name(
            if dev.icon_name.is_empty() { "audio-speakers-symbolic" } else { dev.icon_name }
        );
        icon.set_pixel_size(14);
        row.append(&icon);

        let label = gtk::Label::new(Some(dev.description));
        label.set_hexpand(true);
        label.set_halign(gtk::Align::Start);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_max_width_chars(30);
        row.append(&label);

        if dev.is_default {
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.set_pixel_size(14);
            row.append(&check);
        }

        let btn = gtk::Button::new();
        btn.set_child(Some(&row));
        btn.add_css_class("flat");
        btn.set_tooltip_text(Some(dev.description));

        let c = client.clone();
        let n = dev.name.to_owned();
        let box_ref = devices_box.clone();
        let chevron_ref = chevron.clone();
        btn.connect_clicked(move |_| {
            spawn_call(&c, method, serde_json::json!({"name": n}));
            box_ref.set_visible(false);
            chevron_ref.set_label("›");
        });

        container.append(&btn);
    }
}

fn build_stream_row(parent: &gtk::Box, stream: &AudioStream, max_vol: f64, client: &Arc<Client>) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("stream-row");
    row.set_tooltip_text(Some(&format!("{} — {}%", stream.app_name, stream.volume)));

    let icon_name = if stream.app_icon.is_empty() { "application-x-executable-symbolic" } else { &stream.app_icon };
    let img = gtk::Image::from_icon_name(icon_name);
    img.set_pixel_size(16);
    row.append(&img);

    let name = gtk::Label::new(Some(&stream.app_name));
    name.set_width_chars(10);
    name.set_max_width_chars(12);
    name.set_halign(gtk::Align::Start);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    row.append(&name);

    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, max_vol, 1.0);
    scale.set_value(stream.volume as f64);
    scale.set_hexpand(true);

    let vol = gtk::Label::new(Some(&format!("{}%", stream.volume)));
    vol.set_width_chars(4);
    vol.add_css_class("audio-label");

    let vol_c = vol.clone();
    scale.connect_value_changed(move |s| {
        vol_c.set_label(&format!("{}%", s.value() as u32));
    });

    let c = client.clone();
    let idx = stream.index;
    scale.connect_change_value(move |_, _, val| {
        spawn_call(&c, "audio.set_volume", serde_json::json!({"target": idx.to_string(), "volume": val as u32}));
        glib::Propagation::Proceed
    });

    row.append(&scale);
    row.append(&vol);

    let mute_icon = if stream.muted { "audio-volume-muted-symbolic" } else { "audio-volume-high-symbolic" };
    let mute = gtk::Button::from_icon_name(mute_icon);
    mute.add_css_class("flat");
    let c = client.clone();
    let idx = stream.index;
    mute.connect_clicked(move |_| {
        spawn_call(&c, "audio.set_mute", serde_json::json!({"target": idx.to_string()}));
    });
    row.append(&mute);

    parent.append(&row);
}
