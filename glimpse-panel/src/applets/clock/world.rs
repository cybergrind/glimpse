use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct WorldClock {}

#[derive(Debug)]
pub enum Input {}

#[relm4::component(pub)]
impl SimpleComponent for WorldClock {
    type Init = ();
    type Input = Input;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "card",
            add_css_class: "world-clock",

            gtk::Label {
                add_css_class: "card-heading",
                add_css_class: "world-clock-header",
                set_label: "World Clock",
                set_xalign: 0.0,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "card-body",
                add_css_class: "world-clock-list",

                gtk::Label {
                    add_css_class: "body",
                    add_css_class: "text-muted",
                    add_css_class: "world-clock-empty",
                    set_label: "No locations configured",
                },
            },
        }
    }

    fn init(_: (), _root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = WorldClock {};
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Input, _sender: ComponentSender<Self>) {
        match msg {}
    }
}
