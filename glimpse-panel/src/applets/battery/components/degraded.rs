use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct DegradedWarning {
    text: String,
    visible: bool,
}

#[derive(Debug)]
pub enum DegradedWarningInput {
    Update(String),
}

#[relm4::component(pub)]
impl SimpleComponent for DegradedWarning {
    type Init = ();
    type Input = DegradedWarningInput;
    type Output = ();

    view! {
        gtk::Box {
            add_css_class: "profile-degraded-row",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 6,
            #[watch]
            set_visible: model.visible,

            gtk::Image {
                set_icon_name: Some("dialog-warning-symbolic"),
                set_pixel_size: 14,
            },

            gtk::Label {
                add_css_class: "profile-degraded",
                set_halign: gtk::Align::Start,
                set_wrap: true,
                #[watch]
                set_label: &model.text,
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = DegradedWarning {
            text: String::new(),
            visible: false,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            DegradedWarningInput::Update(reason) => {
                self.visible = !reason.is_empty();
                self.text = if self.visible {
                    format!("Performance degraded: {reason}")
                } else {
                    String::new()
                };
            }
        }
    }
}
