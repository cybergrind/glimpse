use glimpse::providers::audio::AudioDevice;
use relm4::{
    gtk::{self, prelude::*},
    ComponentParts, ComponentSender, SimpleComponent,
};

pub struct DeviceRow {
    device: AudioDevice,
}

#[derive(Debug)]
pub enum DeviceRowInput {
    Update(AudioDevice),
    Pressed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceRowOutput {
    Selected(String),
}

#[relm4::component(pub)]
impl SimpleComponent for DeviceRow {
    type Init = AudioDevice;
    type Input = DeviceRowInput;
    type Output = DeviceRowOutput;

    view! {
        root = gtk::Button {
            add_css_class: "flat",
            #[watch]
            set_tooltip_text: Some(&model.device.description),
            connect_clicked => DeviceRowInput::Pressed,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "device-item",

                gtk::Image {
                    #[watch]
                    set_icon_name: Some(device_icon_name(&model.device)),
                    set_pixel_size: 16,
                    add_css_class: "device-icon",
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.device.description,
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_max_width_chars: 30,
                },

                gtk::Image {
                    set_icon_name: Some("object-select-symbolic"),
                    set_pixel_size: 16,
                    add_css_class: "device-check",
                    #[watch]
                    set_visible: model.device.is_default,
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = DeviceRow { device: init };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            DeviceRowInput::Update(device) => {
                self.device = device;
            }
            DeviceRowInput::Pressed => {
                let _ = sender.output(DeviceRowOutput::Selected(self.device.name.clone()));
            }
        }
    }
}

fn device_icon_name(device: &AudioDevice) -> &str {
    if device.icon_name.is_empty() {
        "audio-speakers-symbolic"
    } else {
        &device.icon_name
    }
}
