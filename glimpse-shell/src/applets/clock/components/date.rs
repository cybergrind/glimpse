#![allow(unused_assignments)]

use chrono::{Local, NaiveDate};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, prelude::*},
};

use crate::applets::clock::format;

pub struct Date {
    weekday: String,
    date: String,
}

#[derive(Debug)]
pub enum DateInput {
    SetDate(NaiveDate),
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for DateView {
    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "date",

            #[name = "weekday"]
            gtk::Label {
                add_css_class: "date-day-of-week",
                set_xalign: 0.0,
            },

            #[name = "date"]
            gtk::Label {
                add_css_class: "date-date",
                set_xalign: 0.0,
            },
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Date {
    type Init = NaiveDate;
    type Input = DateInput;
    type Output = ();

    view! {
        root = gtk::Box {
            #[template]
            DateView {
                #[template_child]
                weekday {
                    #[watch]
                    set_label: &model.weekday,
                },

                #[template_child]
                date {
                    #[watch]
                    set_label: &model.date,
                },
            }
        }
    }

    fn init(
        date: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = Date {
            weekday: String::new(),
            date: String::new(),
        };
        model.set_date(date);
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            DateInput::SetDate(date) => self.set_date(date),
        }
    }
}

impl Date {
    fn set_date(&mut self, date: NaiveDate) {
        let today = Local::now().date_naive();
        self.weekday = if date == today {
            "Today".into()
        } else {
            format::selected_weekday(date)
        };
        self.date = format::selected_date(date);
    }
}
