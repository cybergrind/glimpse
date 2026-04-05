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
    hero_icon: gtk::Image,
    hero_subtitle: gtk::Label,
    output_scale: gtk::Scale,
    output_mute_btn: gtk::Button,
    input_scale: gtk::Scale,
    input_mute_btn: gtk::Button,
    output_device_label: gtk::Label,
    output_device_chevron: gtk::Label,
    output_devices_box: gtk::Box,
    input_device_label: gtk::Label,
    input_device_chevron: gtk::Label,
    input_devices_box: gtk::Box,
    apps_label: gtk::Label,
    apps_chevron: gtk::Label,
    apps_box: gtk::Box,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
    pub client: Arc<Client>,
    pub config: AudioConfig,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateStatus { icon: String, description: String, volume: u32, muted: bool },
    UpdateOutputs(Vec<AudioOutput>),
    UpdateInputs(Vec<AudioInput>),
    UpdateStreams(Vec<AudioStream>),
}

fn spawn_call(client: &Arc<Client>, method: &'static str, params: serde_json::Value) {
    let c = client.clone();
    glib::spawn_future_local(async move { let _ = c.call(method, params).await; });
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
        root.add_css_class("audio-popover");

        let max_vol = init.config.max_volume as f64;
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        // === Hero: icon + title/subtitle ===
        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hero.add_css_class("audio-hero");

        let hero_icon = gtk::Image::from_icon_name("audio-volume-high-symbolic");
        hero_icon.set_pixel_size(32);
        hero.append(&hero_icon);

        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        title_box.set_hexpand(true);
        title_box.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(Some("Audio"));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("audio-hero-title");
        title_box.append(&title);
        let hero_subtitle = gtk::Label::new(None);
        hero_subtitle.set_halign(gtk::Align::Start);
        hero_subtitle.add_css_class("audio-hero-subtitle");
        title_box.append(&hero_subtitle);
        hero.append(&title_box);

        vbox.append(&hero);
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Output slider: [mute icon] [slider] ===
        let output_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        output_row.add_css_class("volume-row");
        let output_mute_btn = gtk::Button::from_icon_name("audio-volume-high-symbolic");
        output_mute_btn.add_css_class("flat");
        output_mute_btn.add_css_class("mute-btn");
        let c = init.client.clone();
        output_mute_btn.connect_clicked(move |_| {
            spawn_call(&c, "audio.set_mute", serde_json::json!({}));
        });
        output_row.append(&output_mute_btn);
        let output_scale = build_scale(max_vol, &init.client, None);
        output_row.append(&output_scale);
        vbox.append(&output_row);

        // === Input slider: [mute icon] [slider] ===
        let input_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        input_row.add_css_class("volume-row");
        let input_mute_btn = gtk::Button::from_icon_name("audio-input-microphone-symbolic");
        input_mute_btn.add_css_class("flat");
        input_mute_btn.add_css_class("mute-btn");
        let c = init.client.clone();
        input_mute_btn.connect_clicked(move |_| {
            spawn_call(&c, "audio.set_mute", serde_json::json!({"target": "@DEFAULT_SOURCE@"}));
        });
        input_row.append(&input_mute_btn);
        let input_scale = build_scale(max_vol, &init.client, Some("@DEFAULT_SOURCE@"));
        input_row.append(&input_scale);
        vbox.append(&input_row);

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Output device selector ===
        let (output_device_label, output_device_chevron, output_devices_box) =
            build_device_row(&vbox, "Output device");

        // === Input device selector ===
        let (input_device_label, input_device_chevron, input_devices_box) =
            build_device_row(&vbox, "Input device");

        // === Apps (collapsible) ===
        let (apps_label, apps_chevron, apps_box) = build_apps_row(&vbox);

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Settings ===
        if !init.config.settings_command.is_empty() {
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

        let model = Popover {
            popover: root.clone(), config: init.config, client: init.client,
            hero_icon, hero_subtitle,
            output_scale, output_mute_btn, input_scale, input_mute_btn,
            output_device_label, output_device_chevron, output_devices_box,
            input_device_label, input_device_chevron, input_devices_box,
            apps_label, apps_chevron, apps_box,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            PopoverInput::Toggle => {
                if self.popover.is_visible() { self.popover.popdown(); }
                else { self.popover.popup(); }
            }
            PopoverInput::UpdateStatus { icon, description, volume, muted } => {
                self.hero_icon.set_icon_name(Some(&icon));
                let label = if muted {
                    format!("{description} — muted")
                } else {
                    format!("{description} — {volume}%")
                };
                self.hero_subtitle.set_label(&label);
            }
            PopoverInput::UpdateOutputs(outputs) => {
                if let Some(d) = outputs.iter().find(|o| o.is_default) {
                    if !is_being_dragged(&self.output_scale) {
                        self.output_scale.set_value(d.volume as f64);
                    }
                    let icon = if d.muted { "audio-volume-muted-symbolic" }
                        else if d.volume == 0 { "audio-volume-muted-symbolic" }
                        else if d.volume < 33 { "audio-volume-low-symbolic" }
                        else if d.volume < 66 { "audio-volume-medium-symbolic" }
                        else { "audio-volume-high-symbolic" };
                    self.output_mute_btn.set_icon_name(icon);
                    self.hero_icon.set_icon_name(Some(icon));
                    self.hero_subtitle.set_label(&format!("{} — {}%", d.description, d.volume));
                    self.output_mute_btn.set_tooltip_text(Some(
                        &format!("{} — {}%", d.description, d.volume)
                    ));
                    self.output_device_label.set_tooltip_text(Some(&d.description));
                }
                rebuild_device_list(
                    &self.output_devices_box, &outputs, &self.client,
                    "audio.set_default_output", true,
                    &self.output_devices_box, &self.output_device_chevron,
                );
            }
            PopoverInput::UpdateInputs(inputs) => {
                if let Some(d) = inputs.iter().find(|i| i.is_default) {
                    if !is_being_dragged(&self.input_scale) {
                        self.input_scale.set_value(d.volume as f64);
                    }
                    self.input_mute_btn.set_icon_name(
                        if d.muted { "microphone-sensitivity-muted-symbolic" }
                        else { "audio-input-microphone-symbolic" }
                    );
                    self.input_mute_btn.set_tooltip_text(Some(
                        &format!("{} — {}%", d.description, d.volume)
                    ));
                    self.input_device_label.set_tooltip_text(Some(&d.description));
                }
                let as_outputs: Vec<AudioOutput> = inputs.iter().map(|i| AudioOutput {
                    name: i.name.clone(), description: i.description.clone(),
                    volume: i.volume, muted: i.muted, is_default: i.is_default,
                    icon_name: "audio-input-microphone-symbolic".into(),
                }).collect();
                rebuild_device_list(
                    &self.input_devices_box, &as_outputs, &self.client,
                    "audio.set_default_input", false,
                    &self.input_devices_box, &self.input_device_chevron,
                );
            }
            PopoverInput::UpdateStreams(streams) => {
                let count = streams.len();
                let visible = self.apps_box.is_visible();
                self.apps_label.set_label(&format!("Apps ({count})"));
                self.apps_chevron.set_label(if visible { "⌄" } else { "›" });

                clear_box(&self.apps_box);
                let max_vol = self.config.max_volume as f64;
                for (i, stream) in streams.iter().enumerate() {
                    build_stream_row(&self.apps_box, stream, max_vol, &self.client, i > 0);
                }
            }
        }
    }
}

fn is_being_dragged(scale: &gtk::Scale) -> bool {
    scale.state_flags().contains(gtk::StateFlags::ACTIVE)
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() { container.remove(&child); }
}

fn build_scale(max_vol: f64, client: &Arc<Client>, target: Option<&str>) -> gtk::Scale {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Instant;

    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, max_vol, 1.0);
    scale.set_hexpand(true);
    if max_vol > 100.0 { scale.add_mark(100.0, gtk::PositionType::Bottom, None); }

    let c = client.clone();
    let t = target.map(|s| s.to_owned());
    let last_sent = Rc::new(Cell::new(Instant::now()));
    let pending = Rc::new(Cell::new(false));

    scale.connect_change_value(move |scale, _, val| {
        let now = Instant::now();
        let elapsed = now.duration_since(last_sent.get());

        if elapsed.as_millis() >= 100 {
            last_sent.set(now);
            let mut params = serde_json::json!({"volume": val as u32});
            if let Some(ref t) = t { params["target"] = serde_json::json!(t); }
            spawn_call(&c, "audio.set_volume", params);
            pending.set(false);
        } else if !pending.get() {
            // Schedule a send after the throttle window.
            pending.set(true);
            let c = c.clone();
            let t = t.clone();
            let last_sent = last_sent.clone();
            let pending = pending.clone();
            let scale = scale.clone();
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(100),
                move || {
                    if pending.get() {
                        pending.set(false);
                        last_sent.set(Instant::now());
                        let val = scale.value() as u32;
                        let mut params = serde_json::json!({"volume": val});
                        if let Some(ref t) = t { params["target"] = serde_json::json!(t); }
                        spawn_call(&c, "audio.set_volume", params);
                    }
                },
            );
        }

        glib::Propagation::Proceed
    });
    scale
}

fn build_device_row(
    parent: &gtk::Box, prefix: &str,
) -> (gtk::Label, gtk::Label, gtk::Box) {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("device-header");

    let label = gtk::Label::new(Some(prefix));
    label.set_hexpand(true);
    label.set_halign(gtk::Align::Start);
    header.append(&label);

    let chevron = gtk::Label::new(Some("›"));
    chevron.add_css_class("chevron");
    header.append(&chevron);

    let btn = gtk::Button::new();
    btn.set_child(Some(&header));
    btn.add_css_class("flat");
    btn.add_css_class("device-btn");
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

    (label, chevron, devices_box)
}

fn build_apps_row(parent: &gtk::Box) -> (gtk::Label, gtk::Label, gtk::Box) {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("device-header");

    let label = gtk::Label::new(Some("Apps"));
    label.set_hexpand(true);
    label.set_halign(gtk::Align::Start);
    header.append(&label);

    let chevron = gtk::Label::new(Some("›"));
    chevron.add_css_class("chevron");
    header.append(&chevron);

    let btn = gtk::Button::new();
    btn.set_child(Some(&header));
    btn.add_css_class("flat");
    btn.add_css_class("device-btn");
    parent.append(&btn);

    let apps_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    apps_box.set_visible(false);
    apps_box.add_css_class("apps-box");
    parent.append(&apps_box);

    let box_ref = apps_box.clone();
    let chevron_ref = chevron.clone();
    btn.connect_clicked(move |_| {
        let show = !box_ref.is_visible();
        box_ref.set_visible(show);
        chevron_ref.set_label(if show { "⌄" } else { "›" });
    });

    (label, chevron, apps_box)
}

fn rebuild_device_list(
    container: &gtk::Box, devices: &[AudioOutput], client: &Arc<Client>,
    method: &'static str, show_icons: bool,
    devices_box: &gtk::Box, chevron: &gtk::Label,
) {
    clear_box(container);
    for dev in devices {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("device-item");

        if show_icons {
            let icon = gtk::Image::from_icon_name(
                if dev.icon_name.is_empty() { "audio-speakers-symbolic" } else { &dev.icon_name }
            );
            icon.set_pixel_size(16);
            icon.add_css_class("device-icon");
            row.append(&icon);
        }

        let label = gtk::Label::new(Some(&dev.description));
        label.set_hexpand(true);
        label.set_halign(gtk::Align::Start);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_max_width_chars(30);
        row.append(&label);

        if dev.is_default {
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.set_pixel_size(16);
            check.add_css_class("device-check");
            row.append(&check);
        }

        let btn = gtk::Button::new();
        btn.set_child(Some(&row));
        btn.add_css_class("flat");
        btn.set_tooltip_text(Some(&dev.description));

        let c = client.clone();
        let n = dev.name.clone();
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

fn build_stream_row(
    parent: &gtk::Box, stream: &AudioStream, max_vol: f64,
    client: &Arc<Client>, show_divider: bool,
) {
    if show_divider {
        let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        sep.add_css_class("stream-divider");
        parent.append(&sep);
    }

    let container = gtk::Box::new(gtk::Orientation::Vertical, 2);
    container.add_css_class("stream-row");
    container.set_tooltip_text(Some(&format!("{} — {}%", stream.app_name, stream.volume)));

    // App name.
    let name = gtk::Label::new(Some(&stream.app_name));
    name.set_halign(gtk::Align::Start);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_max_width_chars(30);
    name.add_css_class("stream-name");
    container.append(&name);

    // [mute icon] [slider]
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

    let mute_icon = if stream.muted { "audio-volume-muted-symbolic" } else { "audio-volume-high-symbolic" };
    let mute = gtk::Button::from_icon_name(mute_icon);
    mute.add_css_class("flat");
    mute.add_css_class("mute-btn");
    let c = client.clone();
    let idx = stream.index;
    mute.connect_clicked(move |_| {
        spawn_call(&c, "audio.set_mute", serde_json::json!({"target": idx.to_string()}));
    });
    row.append(&mute);

    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, max_vol, 1.0);
    scale.set_value(stream.volume as f64);
    scale.set_hexpand(true);
    let c = client.clone();
    let idx = stream.index;
    scale.connect_change_value(move |_, _, val| {
        spawn_call(&c, "audio.set_volume", serde_json::json!({"target": idx.to_string(), "volume": val as u32}));
        glib::Propagation::Proceed
    });
    row.append(&scale);

    container.append(&row);
    parent.append(&container);
}
