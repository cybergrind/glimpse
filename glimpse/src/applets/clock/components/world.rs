#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque},
    gtk::{self, prelude::*},
};

use crate::applets::clock::{
    components::timezone::{TimezoneRow, TimezoneRowInput},
    TimezoneEntry,
};

pub struct WorldClock {
    rows: FactoryVecDeque<TimezoneRowItem>,
}

struct TimezoneRowItem {
    row: Controller<TimezoneRow>,
}

impl FactoryComponent for TimezoneRowItem {
    type Init = TimezoneEntry;
    type Input = TimezoneRowInput;
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        let row = TimezoneRow::builder().launch(init).detach();
        Self { row }
    }

    fn init_root(&self) -> Self::Root {
        self.row.widget().clone()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        _root: Self::Root,
        _returned_widget: &gtk::Widget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
    }

    fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        self.row.emit(msg);
    }
}

#[derive(Debug)]
pub enum WorldClockInput {
    Tick,
}

#[relm4::component(pub)]
impl SimpleComponent for WorldClock {
    type Init = Vec<TimezoneEntry>;
    type Input = WorldClockInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "world-clock",

            gtk::Label {
                add_css_class: "section-title",
                add_css_class: "world-clock-header",
                set_label: "World Clock",
                set_xalign: 0.0,
            },

            #[local_ref]
            list_box -> gtk::Box {
                add_css_class: "world-clock-list",
            },
        }
    }

    fn init(
        timezones: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let mut rows = FactoryVecDeque::builder().launch(list_box.clone()).detach();

        {
            let mut guard = rows.guard();
            for timezone in timezones {
                guard.push_back(timezone);
            }
        }

        let model = WorldClock { rows };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: WorldClockInput, _sender: ComponentSender<Self>) {
        match msg {
            WorldClockInput::Tick => {
                let row_count = self.rows.guard().len();
                for index in 0..row_count {
                    self.rows.send(index, TimezoneRowInput::Tick);
                }
            }
        }
    }
}
