use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct Notifications {}

#[derive(Debug)]
pub enum Input {}

#[relm4::component(pub)]
impl SimpleComponent for Notifications {
    type Init = ();
    type Input = Input;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "notifications",
            set_vexpand: true,

            gtk::Label {
                add_css_class: "notifications-header",
                set_label: "Notifications",
                set_xalign: 0.0,
            },

            gtk::ScrolledWindow {
                set_vexpand: true,
                set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    add_css_class: "notifications-list",

                    gtk::Label {
                        add_css_class: "notifications-empty",
                        set_label: "No notifications",
                        set_vexpand: true,
                        set_valign: gtk::Align::Center,
                    },
                },
            },
        }
    }

    fn init(_: (), _root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = Notifications {};
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Input, _sender: ComponentSender<Self>) {
        match msg {}
    }
}
