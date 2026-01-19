use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::AppletInstance;

pub struct AppletHostInit {
    pub applet: AppletInstance,
}

#[derive(Debug)]
pub enum Input {
    Click(MouseButton),
    Scroll(ScrollDirection),
}

pub struct AppletHost {
    #[allow(dead_code)]
    applet: AppletInstance,
}

#[relm4::component(pub)]
impl SimpleComponent for AppletHost {
    type Init = AppletHostInit;
    type Input = Input;
    type Output = ();

    view! {
        gtk::Box {
            add_css_class: "applet",
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widget = init.applet.widget();
        root.append(&widget);

        let sender_clone = sender.clone();
        let click_controller = gtk::GestureClick::new();
        click_controller.set_button(0);
        let click_controller_clone = click_controller.clone();
        click_controller.connect_released(move |_, _, _, _| {
            let button = click_controller_clone.current_button();
            if let Some(btn) = MouseButton::from_button(button) {
                sender_clone.input(Input::Click(btn));
            }
        });
        widget.add_controller(click_controller);

        let scroll_sender = sender.clone();
        let scroll_controller =
            gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        scroll_controller.connect_scroll(move |_, _, dy| {
            if dy > 0.0 {
                scroll_sender.input(Input::Scroll(ScrollDirection::Down));
            } else {
                scroll_sender.input(Input::Scroll(ScrollDirection::Up));
            }
            gtk::glib::Propagation::Stop
        });
        widget.add_controller(scroll_controller);

        let model = AppletHost {
            applet: init.applet,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Input::Click(btn) => self.applet.applet.on_click(btn),
            Input::Scroll(direction) => self.applet.applet.on_scroll(direction),
        }
    }
}

#[derive(Debug)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

impl MouseButton {
    fn from_button(button: u32) -> Option<Self> {
        match button {
            1 => Some(MouseButton::Left),
            2 => Some(MouseButton::Middle),
            3 => Some(MouseButton::Right),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum ScrollDirection {
    Up,
    Down,
}
