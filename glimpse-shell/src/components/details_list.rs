use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, prelude::*},
};

pub struct DetailsList {
    value: Vec<Value>,
}

#[derive(Debug)]
pub struct Value {
    pub label: String,
    pub value: String,
}

#[derive(Debug)]
pub struct Init {
    pub values: Vec<Value>,
}

#[derive(Debug)]
pub enum Input {
    Update(Vec<Value>),
}

#[relm4::widget_template]
impl WidgetTemplate for DetailRow {
    view! {
            gtk::Box {
                add_css_class: "detail-grid__row",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Label {
                    add_css_class: "detail-grid__key",
                    set_label: "Health",
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                },

                gtk::Label {
                    add_css_class: "detail-grid__value",
                },
            },
    }
}

#[relm4::component(pub)]
impl SimpleComponent for DetailsList {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "detail-grid",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = DetailsList { value: init.values };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Update(values) => self.value = values,
        }
    }
}
