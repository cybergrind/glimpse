use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacyIndicatorsState {
    pub visible: bool,
    pub mic_active: bool,
    pub camera_active: bool,
    pub screen_capture_active: bool,
    pub recording_label: String,
}

pub struct PrivacyIndicators {
    state: PrivacyIndicatorsState,
}

#[derive(Debug)]
pub enum PrivacyIndicatorsInput {
    Update(PrivacyIndicatorsState),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacyIndicatorsOutput {
    StopScreenCaptureRequested,
}

#[relm4::component(pub)]
impl SimpleComponent for PrivacyIndicators {
    type Init = ();
    type Input = PrivacyIndicatorsInput;
    type Output = PrivacyIndicatorsOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 6,
            add_css_class: "applet",
            add_css_class: "privacy",
            #[watch]
            set_visible: model.state.visible,

            gtk::Image {
                set_icon_name: Some("microphone-sensitivity-high-symbolic"),
                set_pixel_size: 16,
                add_css_class: "privacy-indicator",
                #[watch]
                set_visible: model.state.mic_active,
            },

            gtk::Image {
                set_icon_name: Some("camera-web-symbolic"),
                set_pixel_size: 16,
                add_css_class: "privacy-indicator",
                #[watch]
                set_visible: model.state.camera_active,
            },

            gtk::Button {
                add_css_class: "flat",
                add_css_class: "privacy-recording-pill",
                #[watch]
                set_visible: model.state.screen_capture_active,
                connect_clicked[sender] => move |_| {
                    let _ = sender.output(PrivacyIndicatorsOutput::StopScreenCaptureRequested);
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,

                    gtk::Image {
                        set_icon_name: Some("media-record-symbolic"),
                        set_pixel_size: 14,
                        add_css_class: "privacy-recording-icon",
                    },

                    gtk::Label {
                        add_css_class: "privacy-recording-label",
                        #[watch]
                        set_label: &model.state.recording_label,
                    },
                },
            },
        }
    }

    fn init(_: (), _root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = PrivacyIndicators {
            state: PrivacyIndicatorsState::default(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let PrivacyIndicatorsInput::Update(state) = msg;
        self.state = state;
    }
}
