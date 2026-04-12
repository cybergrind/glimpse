use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct AudioHero {
    icon_name: String,
    subtitle: String,
}

#[derive(Debug)]
pub enum AudioHeroInput {
    Update { icon_name: String, subtitle: String },
}

#[relm4::component(pub)]
impl SimpleComponent for AudioHero {
    type Init = ();
    type Input = AudioHeroInput;
    type Output = ();

    view! {
        gtk::Box {
            set_spacing: 12,
            add_css_class: "audio-hero",

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 32,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,
                set_valign: gtk::Align::Center,

                gtk::Label {
                    set_label: "Audio",
                    set_halign: gtk::Align::Start,
                    add_css_class: "audio-hero-title",
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.subtitle,
                    set_halign: gtk::Align::Start,
                    add_css_class: "audio-hero-subtitle",
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = AudioHero {
            icon_name: "audio-volume-high-symbolic".into(),
            subtitle: String::new(),
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let AudioHeroInput::Update {
            icon_name,
            subtitle,
        } = msg;
        self.icon_name = icon_name;
        self.subtitle = subtitle;
    }
}
