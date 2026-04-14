use glimpse::network::provider::NetworkDevice;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct WiredRow {
    device: NetworkDevice,
    info: String,
}

#[derive(Debug)]
pub enum WiredRowInput {
    Update(NetworkDevice),
}

#[relm4::component(pub)]
impl SimpleComponent for WiredRow {
    type Init = NetworkDevice;
    type Input = WiredRowInput;
    type Output = ();

    view! {
        gtk::Button {
            add_css_class: "flat",
            add_css_class: "net-device-btn",
            add_css_class: "action-row",
            add_css_class: "action-row__button",
            set_sensitive: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "action-row__content-shell",

                gtk::Image {
                    set_icon_name: Some("network-wired-symbolic"),
                    set_pixel_size: 16,
                    set_valign: gtk::Align::Center,
                    add_css_class: "net-ap-icon",
                    add_css_class: "action-row__leading",
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.device.interface,
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    add_css_class: "action-row__title",
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.info,
                    add_css_class: "net-dim",
                    add_css_class: "action-row__meta",
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = WiredRow {
            info: wired_info(&init),
            device: init,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let WiredRowInput::Update(device) = message;
        self.info = wired_info(&device);
        self.device = device;
    }
}

fn wired_info(device: &NetworkDevice) -> String {
    if device.state == "connected" {
        if device.speed > 0 {
            format!("{} Mbps", device.speed)
        } else {
            "Connected".into()
        }
    } else if device.carrier.unwrap_or(false) {
        "Cable connected".into()
    } else {
        "Disconnected".into()
    }
}
