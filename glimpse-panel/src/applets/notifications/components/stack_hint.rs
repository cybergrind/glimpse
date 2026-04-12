use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct StackHint {
    second: gtk::Box,
    third: gtk::Box,
}

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
        root = gtk::Overlay {
            add_css_class: "notif-stack-backplates",

            #[name(third)]
            gtk::Box {
                add_css_class: "notif-stack-backplate",
                add_css_class: "notif-stack-backplate-lower",
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Start,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let second = gtk::Box::new(gtk::Orientation::Vertical, 0);
        second.add_css_class("notif-stack-backplate");
        second.add_css_class("notif-stack-backplate-second");
        second.set_halign(gtk::Align::Fill);
        second.set_valign(gtk::Align::Start);

        let widgets = view_output!();
        widgets.root.add_overlay(&second);
        widgets.root.set_measure_overlay(&second, true);

        let mut model = StackHint {
            second,
            third: widgets.third.clone(),
        };
        model.set_depth(init.depth);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let StackHintInput::SetDepth(depth) = msg;
        self.set_depth(depth);
    }
}

impl StackHint {
    fn set_depth(&mut self, depth: usize) {
        self.second.set_visible(depth >= 1);
        self.third.set_visible(depth >= 2);
    }
}
