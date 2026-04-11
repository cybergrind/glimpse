use adw::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, gdk};
use relm4::prelude::*;

use glimpse::wallpaper::WallpaperConfig;

use super::color_widget::ColorWidget;

pub struct MonitorWindowInit {
    pub monitor: gdk::Monitor,
    pub config: WallpaperConfig,
}

pub struct MonitorWindow {
    #[allow(dead_code)]
    content: Controller<ColorWidget>,
}

#[relm4::component(pub)]
impl SimpleComponent for MonitorWindow {
    type Init = MonitorWindowInit;
    type Input = ();
    type Output = ();

    view! {
        gtk::Window {
            set_decorated: false,
        }
    }

    fn init(init: Self::Init, root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        setup_layer_shell(&root, &init.monitor);

        let content = ColorWidget::builder()
            .launch(init.config.color.clone())
            .detach();

        root.set_child(Some(content.widget()));
        root.present();

        let model = MonitorWindow { content };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: (), _sender: ComponentSender<Self>) {}
}

fn setup_layer_shell(window: &gtk::Window, monitor: &gdk::Monitor) {
    window.init_layer_shell();
    window.set_layer(Layer::Background);
    window.set_namespace("glimpse-wallpaper");
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_exclusive_zone(-1);
    window.set_monitor(monitor);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_decorated(false);
}
