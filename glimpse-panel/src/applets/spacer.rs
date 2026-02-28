use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct Spacer;

#[derive(Debug)]
pub enum Input {}

#[relm4::component(pub)]
impl SimpleComponent for Spacer {
    type Init = ();
    type Input = Input;
    type Output = ();

    view! {
        gtk::Box {
            set_hexpand: true,
        }
    }

    fn init(_: (), _root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = Spacer;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}
