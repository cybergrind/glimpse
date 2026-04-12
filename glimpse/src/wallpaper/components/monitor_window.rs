use adw::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, gdk};
use relm4::prelude::*;

use crate::wallpaper::{WallpaperConfig, WallpaperMode};

use super::color_widget::ColorWidget;
use super::image_widget::{ImageWidget, ImageWidgetInit};

pub struct MonitorWindowInit {
    pub monitor: gdk::Monitor,
    pub config: WallpaperConfig,
}

enum Content {
    Color(Controller<ColorWidget>),
    Image(Controller<ImageWidget>),
}

impl Content {
    fn widget(&self) -> gtk::Widget {
        match self {
            Content::Color(c) => c.widget().clone().upcast(),
            Content::Image(c) => c.widget().clone().upcast(),
        }
    }
}

pub struct MonitorWindow {
    _content: Content,
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

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        setup_layer_shell(&root, &init.monitor);

        let widgets = view_output!();

        let content = launch_content(&init.config);
        root.set_child(Some(&content.widget()));

        root.present();

        let model = MonitorWindow { _content: content };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {
    }
}

fn launch_content(config: &WallpaperConfig) -> Content {
    match config.mode {
        WallpaperMode::Color => {
            tracing::info!(mode = "color", color = %config.color, "launching wallpaper");
            Content::Color(ColorWidget::builder().launch(config.color.clone()).detach())
        }
        WallpaperMode::Image => {
            let path = config.path.clone().unwrap_or_default();
            tracing::info!(mode = "image", path = %path.display(), fit = ?config.fit, "launching wallpaper");
            Content::Image(
                ImageWidget::builder()
                    .launch(ImageWidgetInit {
                        path,
                        fit: config.fit.clone(),
                    })
                    .detach(),
            )
        }
    }
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
