use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::{self, prelude::*}};

pub struct StackHint {
    depth: usize,
}

#[derive(Debug)]
pub struct StackHintInit {
    pub depth: usize,
}

#[derive(Debug)]
pub enum StackHintInput {
    SetDepth(usize),
}

#[relm4::component(pub)]
impl SimpleComponent for StackHint {
    type Init = StackHintInit;
    type Input = StackHintInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "notif-stack-shell",

            gtk::Box {
                #[watch]
                set_visible: model.depth >= 1,
                set_orientation: gtk::Orientation::Horizontal,
                add_css_class: "notif-stack-depth",
            },

            gtk::Box {
                #[watch]
                set_visible: model.depth >= 2,
                set_orientation: gtk::Orientation::Horizontal,
                add_css_class: "notif-stack-depth-2",
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = StackHint { depth: init.depth };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let StackHintInput::SetDepth(depth) = msg;
        self.depth = depth;
    }
}

#[cfg(test)]
mod tests {
    use super::{StackHint, StackHintInit, StackHintInput};
    use relm4::{Component, ComponentController};

    #[test]
    fn stack_hint_accepts_depth_updates() {
        let _ = relm4::gtk::init();
        let ctrl = StackHint::builder()
            .launch(StackHintInit { depth: 1 })
            .detach();
        ctrl.emit(StackHintInput::SetDepth(2));
    }
}
