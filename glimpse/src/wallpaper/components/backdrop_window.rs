use std::path::PathBuf;

use crate::config::ImageFit;
use crate::wallpaper::components::image_widget::{ImageWidget, ImageWidgetInit};
use gtk4::prelude::GtkWindowExt;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, gdk};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
};

pub struct BackdropWindowInit {
    pub monitor: gdk::Monitor,
    pub path: PathBuf,
    pub blur_radius: u32,
}

pub struct BackdropWindow {
    _widget: Controller<ImageWidget>,
}

#[relm4::component(pub)]
impl SimpleComponent for BackdropWindow {
    type Init = BackdropWindowInit;
    type Input = ();
    type Output = ();

    view! {
        gtk::Window {
            set_decorated: false,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        setup_layer_shell(&root, &init.monitor);

        let widgets = view_output!();

        let content = ImageWidget::builder()
            .launch(ImageWidgetInit {
                path: init.path,
                fit: ImageFit::Cover,
            })
            .detach();

        root.set_child(Some(content.widget()));
        root.present();

        let model = BackdropWindow { _widget: content };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {}
}

fn setup_layer_shell(window: &gtk::Window, monitor: &gdk::Monitor) {
    window.init_layer_shell();
    window.set_layer(Layer::Background);
    window.set_namespace("glimpse-backdrop");
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_exclusive_zone(-1);
    window.set_monitor(monitor);
    window.set_decorated(false);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
}
