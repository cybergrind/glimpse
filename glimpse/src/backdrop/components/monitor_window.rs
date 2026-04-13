use adw::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, gdk};
use relm4::prelude::*;

use crate::backdrop::{self, BackdropConfig};

use super::image_widget::{BackdropImageWidget, BackdropImageWidgetInit, BackdropImageWidgetMsg};

pub struct BackdropWindowInit {
    pub monitor: gdk::Monitor,
    pub config: BackdropConfig,
}

pub struct BackdropWindow {
    window: gtk::Window,
    monitor: gdk::Monitor,
    content: Controller<BackdropImageWidget>,
}

#[derive(Debug, Clone)]
pub enum BackdropWindowInput {
    Reconfigure(BackdropConfig),
}

#[relm4::component(pub)]
impl SimpleComponent for BackdropWindow {
    type Init = BackdropWindowInit;
    type Input = BackdropWindowInput;
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
                path: init.config.path.clone().unwrap_or_default(),
                width: geometry.width(),
                height: geometry.height(),
                blur_radius: init.config.blur_radius,
            })
            .detach();

        root.set_child(Some(content.widget()));
        root.present();
        apply_backdrop_visibility(&root, &init.config);
        if !backdrop::is_active_config(&init.config) {
            content.emit(BackdropImageWidgetMsg::Clear);
        }

        let model = BackdropWindow {
            window: root.clone(),
            monitor: init.monitor,
            content,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BackdropWindowInput::Reconfigure(config) => {
                apply_backdrop_visibility(&self.window, &config);
                if is_active_backdrop_config(&config) {
                    let geometry = self.monitor.geometry();
                    self.content.emit(BackdropImageWidgetMsg::Reconfigure(
                        BackdropImageWidgetInit {
                            path: config.path.unwrap_or_default(),
                            width: geometry.width(),
                            height: geometry.height(),
                            blur_radius: config.blur_radius,
                        },
                    ));
                } else {
                    self.content.emit(BackdropImageWidgetMsg::Clear);
                }
            }
        }
    }
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

fn is_active_backdrop_config(config: &BackdropConfig) -> bool {
    config.enabled
        && detect_compositor().capabilities().backdrop
        && config.path.as_ref().is_some_and(|path| path.is_file())
}

fn apply_backdrop_visibility(window: &gtk::Window, config: &BackdropConfig) {
    window.set_visible(is_active_backdrop_config(config));
}
