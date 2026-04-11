use adw::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, gdk};
use relm4::prelude::*;

use glimpse::wallpaper::{WallpaperConfig, WallpaperMode};

use super::color_widget::ColorWidget;
use super::image_widget::{ImageWidget, ImageWidgetInit};

pub struct MonitorWindowInit {
    pub monitor: gdk::Monitor,
    pub config: WallpaperConfig,
}

#[derive(Debug)]
pub enum MonitorWindowMsg {
    ConfigChanged(WallpaperConfig),
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
    stack: gtk::Stack,
    slot_a: gtk::Box,
    slot_b: gtk::Box,
    active: Slot,
    content: Content,
}

#[derive(Debug, Clone, Copy)]
enum Slot {
    A,
    B,
}

impl Slot {
    fn name(self) -> &'static str {
        match self {
            Slot::A => "a",
            Slot::B => "b",
        }
    }

    fn other(self) -> Self {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for MonitorWindow {
    type Init = MonitorWindowInit;
    type Input = MonitorWindowMsg;
    type Output = ();

    view! {
        gtk::Window {
            set_decorated: false,

            #[name = "stack"]
            gtk::Stack {
                set_transition_type: gtk::StackTransitionType::Crossfade,
                set_transition_duration: init.config.transition_ms,
                set_hexpand: true,
                set_vexpand: true,
            }
        }
    }

    fn init(init: Self::Init, root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        setup_layer_shell(&root, &init.monitor);

        let widgets = view_output!();

        let slot_a = make_slot();
        let slot_b = make_slot();
        widgets.stack.add_named(&slot_a, Some("a"));
        widgets.stack.add_named(&slot_b, Some("b"));

        let content = launch_content(&init.config);
        slot_a.append(&content.widget());
        widgets.stack.set_visible_child_name("a");

        root.present();

        let model = MonitorWindow {
            stack: widgets.stack.clone(),
            slot_a,
            slot_b,
            active: Slot::A,
            content,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            MonitorWindowMsg::ConfigChanged(config) => {
                let inactive = self.active.other();
                let inactive_box = match inactive {
                    Slot::A => &self.slot_a,
                    Slot::B => &self.slot_b,
                };

                while let Some(child) = inactive_box.first_child() {
                    inactive_box.remove(&child);
                }

                let content = launch_content(&config);
                inactive_box.append(&content.widget());

                self.stack.set_visible_child_name(inactive.name());
                self.content = content;
                self.active = inactive;
            }
        }
    }
}

fn make_slot() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
    b.set_hexpand(true);
    b.set_vexpand(true);
    b
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
                    .launch(ImageWidgetInit { path, fit: config.fit.clone() })
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
