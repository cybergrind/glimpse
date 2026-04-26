use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, Default)]
pub struct PopoverShellInit {}

pub struct PopoverShell {}

#[derive(Debug)]
pub enum PopoverShellInput {}

#[relm4::component(pub)]
impl SimpleComponent for PopoverShell {
    type Init = PopoverShellInit;
    type Input = PopoverShellInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "popover-shell",

            #[name(content)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "popover-shell__content",
            },

            #[name(footer)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "popover-shell__footer",
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = PopoverShell {};
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, _message: Self::Input, _sender: ComponentSender<Self>) {}
}
