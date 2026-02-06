use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::applets::clock::config::TimezoneEntry;

use super::timezone::TimezoneRow;

pub struct WorldClock {
    #[allow(dead_code)]
    rows: Vec<Controller<TimezoneRow>>,
}

#[derive(Debug)]
pub enum Input {}

#[relm4::component(pub)]
impl SimpleComponent for WorldClock {
    type Init = Vec<TimezoneEntry>;
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

            #[name = "list"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "card-body",
                add_css_class: "world-clock-list",
                set_spacing: 2,
            },
        }
    }

    fn init(
        timezones: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();

        let rows: Vec<_> = timezones
            .into_iter()
            .map(|entry| {
                let row = TimezoneRow::builder().launch(entry).detach();
                widgets.list.append(row.widget());
                row
            })
            .collect();

        let model = WorldClock { rows };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Input, _sender: ComponentSender<Self>) {
        match msg {}
    }
}
