use adw::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, gdk};
use relm4::prelude::*;

use super::image_widget::{BackdropImageWidget, BackdropImageWidgetInit};

pub struct BackdropWindowInit {
    pub monitor: gdk::Monitor,
    pub path: std::path::PathBuf,
    pub blur_radius: u32,
}

pub struct BackdropWindow {
    _content: Controller<BackdropImageWidget>,
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

        let geometry = init.monitor.geometry();
        let content = BackdropImageWidget::builder()
            .launch(BackdropImageWidgetInit {
                path: init.path,
                width: geometry.width(),
                height: geometry.height(),
                blur_radius: init.blur_radius,
            })
            .detach();

        root.set_child(Some(content.widget()));
        root.present();

        let model = BackdropWindow { _content: content };
        let widgets = view_output!();
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
    window.set_deletable(false);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
}
