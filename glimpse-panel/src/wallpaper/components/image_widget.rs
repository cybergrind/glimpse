use std::path::PathBuf;

use adw::prelude::*;
use relm4::gtk::{self, gio};
use relm4::prelude::*;

use glimpse::wallpaper::ImageFit;

pub struct ImageWidgetInit {
    pub path: PathBuf,
    pub fit: ImageFit,
}

pub struct ImageWidget;

#[relm4::component(pub)]
impl SimpleComponent for ImageWidget {
    type Init = ImageWidgetInit;
    type Input = ();
    type Output = ();

    view! {
        gtk::Picture {
            set_hexpand: true,
            set_vexpand: true,
            set_can_shrink: true,
            set_content_fit: init.fit.to_gtk(),
        }
    }

    fn init(init: Self::Init, root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let widgets = view_output!();
        root.set_file(Some(&gio::File::for_path(&init.path)));
        let model = ImageWidget;
        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: (), _sender: ComponentSender<Self>) {}
}
