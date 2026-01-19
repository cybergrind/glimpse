use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct Date {
    day_of_week: String,
    date: String,
}

#[derive(Debug)]
pub enum Input {}

#[relm4::component(pub)]
impl SimpleComponent for Date {
    type Init = ();
    type Input = Input;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "date",

            gtk::Label {
                add_css_class: "date-day-of-week",
                set_xalign: 0.0,
                #[watch]
                set_label: &model.day_of_week,
            },

            gtk::Label {
                add_css_class: "date-date",
                set_xalign: 0.0,
                #[watch]
                set_label: &model.date,
            },
        }
    }

    fn init(_: (), _root: Self::Root, _sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let now = chrono::Local::now();
        let model = Date {
            day_of_week: now.format("%A").to_string(),
            date: now.format("%-d %b, %Y").to_string(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Input, _sender: ComponentSender<Self>) {
        match msg {}
    }
}
