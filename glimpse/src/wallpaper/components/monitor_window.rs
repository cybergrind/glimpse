use adw::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, gdk};
use relm4::prelude::*;

use crate::wallpaper::WallpaperConfig;

use super::color_widget::{ColorWidget, ColorWidgetInput};
use super::image_widget::{ImageWidget, ImageWidgetInit, ImageWidgetMsg};

pub struct MonitorWindowInit {
    pub monitor: gdk::Monitor,
    pub config: WallpaperConfig,
}

pub struct MonitorWindow {
    color: Controller<ColorWidget>,
    image: Controller<ImageWidget>,
}

#[derive(Debug, Clone)]
pub enum MonitorWindowInput {
    Reconfigure(WallpaperConfig),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for MonitorWindow {
    type Init = MonitorWindowInit;
    type Input = MonitorWindowInput;
    type Output = ();

    view! {
        gtk::Window {
            set_decorated: false,

            #[name(overlay)]
            gtk::Overlay {
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        setup_layer_shell(&root, &init.monitor);

        tracing::info!(
            color = %init.config.color,
            path = init
                .config
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<none>".into()),
            fit = ?init.config.fit,
            "launching wallpaper"
        );

        let color = ColorWidget::builder().launch(init.config.color.clone()).detach();
        let image = ImageWidget::builder()
            .launch(ImageWidgetInit {
                path: init.config.path.clone(),
                fit: init.config.fit.clone(),
                transition_ms: init.config.transition_ms,
            })
            .detach();
        let color_widget = color.widget().clone().upcast::<gtk::Widget>();
        let image_widget = image.widget().clone().upcast::<gtk::Widget>();
        let widgets = view_output!();
        widgets.overlay.set_child(Some(&color_widget));
        widgets.overlay.add_overlay(&image_widget);
        root.present();

        let model = MonitorWindow { color, image };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            MonitorWindowInput::Reconfigure(config) => self.reconfigure(config),
        }
    }
}

impl MonitorWindow {
    fn reconfigure(&mut self, config: WallpaperConfig) {
        self.color.emit(ColorWidgetInput::SetColor(config.color.clone()));
        self.image.emit(ImageWidgetMsg::Reconfigure(ImageWidgetInit {
            path: config.path,
            fit: config.fit,
            transition_ms: config.transition_ms,
        }));
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
