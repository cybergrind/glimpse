use adw::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, gdk};
use relm4::prelude::*;

use crate::wallpaper::{WallpaperConfig, WallpaperMode};

use super::color_widget::{ColorWidget, ColorWidgetInput};
use super::image_widget::{ImageWidget, ImageWidgetInit, ImageWidgetMsg};

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
    window: gtk::Window,
    content: Content,
}

#[derive(Debug, Clone)]
pub enum MonitorWindowInput {
    Reconfigure(WallpaperConfig),
}

#[relm4::component(pub)]
impl SimpleComponent for MonitorWindow {
    type Init = MonitorWindowInit;
    type Input = MonitorWindowInput;
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

        let model = MonitorWindow {
            window: root.clone(),
            content,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            MonitorWindowInput::Reconfigure(config) => self.reconfigure(config),
        }
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

impl MonitorWindow {
    fn reconfigure(&mut self, config: WallpaperConfig) {
        match (&self.content, config.mode.clone()) {
            (Content::Color(color), WallpaperMode::Color) => {
                color.emit(ColorWidgetInput::SetColor(config.color));
            }
            (Content::Image(image), WallpaperMode::Image) => {
                let path = config.path.unwrap_or_default();
                image.emit(ImageWidgetMsg::Reconfigure(ImageWidgetInit {
                    path,
                    fit: config.fit,
                }));
            }
            _ => {
                let content = launch_content(&config);
                self.window.set_child(Some(&content.widget()));
                self.content = content;
            }
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
    window.set_decorated(false);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
}
