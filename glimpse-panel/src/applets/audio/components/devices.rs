use glimpse::providers::audio::{AudioProvider, DeviceList};
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::{self, glib, prelude::*}};

pub struct DeviceSection {
    output_list: gtk::Box,
    output_chevron: gtk::Label,
    input_list: gtk::Box,
    input_chevron: gtk::Label,
}

pub struct DeviceSectionInit;

#[derive(Debug)]
pub enum DeviceSectionInput {
    UpdateOutputs(DeviceList),
    UpdateInputs(DeviceList),
    SetDefaultOutput(String),
    SetDefaultInput(String),
}

impl SimpleComponent for DeviceSection {
    type Init = DeviceSectionInit;
    type Input = DeviceSectionInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (_, output_list, output_chevron) = build_section(&root, "Output device");
        let (_, input_list, input_chevron) = build_section(&root, "Input device");

        let model = DeviceSection { output_list, output_chevron, input_list, input_chevron };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            DeviceSectionInput::UpdateOutputs(outputs) => {
                rebuild_list(
                    &self.output_list,
                    &self.output_chevron,
                    &outputs,
                    _sender,
                    DeviceSectionInput::SetDefaultOutput,
                );
            }
            DeviceSectionInput::UpdateInputs(inputs) => {
                rebuild_list(
                    &self.input_list,
                    &self.input_chevron,
                    &inputs,
                    _sender,
                    DeviceSectionInput::SetDefaultInput,
                );
            }
            DeviceSectionInput::SetDefaultOutput(name) => {
                glib::spawn_future_local(async move {
                    if let Err(e) = AudioProvider::new().set_default_output(&name).await {
                        tracing::warn!("audio: set_default_output: {e}");
                    }
                });
            }
            DeviceSectionInput::SetDefaultInput(name) => {
                glib::spawn_future_local(async move {
                    if let Err(e) = AudioProvider::new().set_default_input(&name).await {
                        tracing::warn!("audio: set_default_input: {e}");
                    }
                });
            }
        }
    }
}

fn build_section(parent: &gtk::Box, label: &str) -> (gtk::Box, gtk::Box, gtk::Label) {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
    parent.append(&section);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("device-header");

    let lbl = gtk::Label::new(Some(label));
    lbl.set_hexpand(true);
    lbl.set_halign(gtk::Align::Start);
    header.append(&lbl);

    let chevron = gtk::Label::new(Some("›"));
    chevron.add_css_class("chevron");
    header.append(&chevron);

    let btn = gtk::Button::new();
    btn.set_child(Some(&header));
    btn.add_css_class("flat");
    btn.add_css_class("device-btn");
    section.append(&btn);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.set_visible(false);
    list.add_css_class("device-list");
    section.append(&list);

    let list_ref = list.clone();
    let chevron_ref = chevron.clone();
    btn.connect_clicked(move |_| {
        let show = !list_ref.is_visible();
        list_ref.set_visible(show);
        chevron_ref.set_label(if show { "⌄" } else { "›" });
    });

    (section, list, chevron)
}

fn rebuild_list(
    list: &gtk::Box,
    chevron: &gtk::Label,
    devices: &DeviceList,
    sender: ComponentSender<DeviceSection>,
    make_msg: fn(String) -> DeviceSectionInput,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for dev in devices.iter() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("device-item");

        let icon = gtk::Image::from_icon_name(if dev.icon_name.is_empty() {
            "audio-speakers-symbolic"
        } else {
            &dev.icon_name
        });
        icon.set_pixel_size(16);
        icon.add_css_class("device-icon");
        row.append(&icon);

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

        let name = dev.name.clone();
        let list_ref = list.clone();
        let chevron_ref = chevron.clone();
        let sender = sender.clone();
        btn.connect_clicked(move |_| {
            list_ref.set_visible(false);
            chevron_ref.set_label("›");
            sender.input(make_msg(name.clone()));
        });

        list.append(&btn);
    }
}
