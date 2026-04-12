use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyboardLayoutView {
    pub label: String,
    pub tooltip: String,
}

pub struct KeyboardLayoutLabel {
    view: KeyboardLayoutView,
}

#[derive(Debug)]
pub enum KeyboardLayoutLabelInput {
    Update(KeyboardLayoutView),
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeyboardLayoutLabelOutput {
    Scroll(f64),
}

#[relm4::component(pub)]
impl SimpleComponent for KeyboardLayoutLabel {
    type Init = ();
    type Input = KeyboardLayoutLabelInput;
    type Output = KeyboardLayoutLabelOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "applet",
            add_css_class: "keyboard",
            #[watch]
            set_tooltip_text: if model.view.tooltip.is_empty() {
                None
            } else {
                Some(&model.view.tooltip)
            },

            gtk::Label {
                add_css_class: "keyboard-label",
                #[watch]
                set_label: &model.view.label,
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll(move |_, _, dy| {
            let _ = sender.output(KeyboardLayoutLabelOutput::Scroll(dy));
            gtk::glib::Propagation::Stop
        });
        root.add_controller(scroll);

        let model = KeyboardLayoutLabel {
            view: KeyboardLayoutView::default(),
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let KeyboardLayoutLabelInput::Update(view) = msg;
        self.view = view;
    }
}
