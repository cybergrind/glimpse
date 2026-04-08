use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct AudioHero {
    icon: gtk::Image,
    subtitle: gtk::Label,
}

#[derive(Debug)]
pub enum AudioHeroInput {
    Update { icon_name: String, subtitle: String },
}

impl SimpleComponent for AudioHero {
    type Init = ();
    type Input = AudioHeroInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 12)
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.add_css_class("audio-hero");

        let icon = gtk::Image::from_icon_name("audio-volume-high-symbolic");
        icon.set_pixel_size(32);
        root.append(&icon);

        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text_box.set_hexpand(true);
        text_box.set_valign(gtk::Align::Center);

        let title = gtk::Label::new(Some("Audio"));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("audio-hero-title");
        text_box.append(&title);

        let subtitle = gtk::Label::new(None);
        subtitle.set_halign(gtk::Align::Start);
        subtitle.add_css_class("audio-hero-subtitle");
        text_box.append(&subtitle);

        root.append(&text_box);

        ComponentParts {
            model: AudioHero { icon, subtitle },
            widgets: (),
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let AudioHeroInput::Update {
            icon_name,
            subtitle,
        } = msg;
        self.icon.set_icon_name(Some(&icon_name));
        self.subtitle.set_label(&subtitle);
    }
}
