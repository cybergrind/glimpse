#![allow(unused_assignments)]

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, prelude::*},
};

use crate::{components::section_header::SectionHeader, services::clock::WorldClockTime};

pub struct WorldClock {
    rows: Vec<WorldClockTime>,
    row_views: Vec<WorldClockRowView>,
    visible: bool,
    list_box: gtk::Box,
}

#[derive(Debug)]
pub enum WorldClockInput {
    Update(Vec<WorldClockTime>),
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for WorldClockRowView {
    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            add_css_class: "world-clock-timezone",

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_hexpand: true,
                set_spacing: 6,
                set_valign: gtk::Align::Center,

                #[name = "name"]
                gtk::Label {
                    add_css_class: "world-clock-city",
                    add_css_class: "action-row__title",
                    set_xalign: 0.0,
                },

                #[name = "day"]
                gtk::Label {
                    add_css_class: "caption",
                    add_css_class: "dim-label",
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 6,
                set_valign: gtk::Align::Center,
                set_halign: gtk::Align::End,

                #[name = "time"]
                gtk::Label {
                    add_css_class: "world-clock-time",
                    add_css_class: "detail-grid__value",
                },

                #[name = "offset"]
                gtk::Label {
                    add_css_class: "world-clock-tz",
                    add_css_class: "dim-label",
                    add_css_class: "detail-grid__key",
                },
            },
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for WorldClock {
    type Init = Vec<WorldClockTime>;
    type Input = WorldClockInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "world-clock",
            #[watch]
            set_visible: model.visible,

            #[template]
            SectionHeader {
                add_css_class: "world-clock-header",

                #[template_child]
                title {
                    set_label: "World Clock",
                },
            },

            #[local_ref]
            list_box -> gtk::Box {
                add_css_class: "world-clock-list",
            },
        }
    }

    fn init(
        rows: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let model = WorldClock {
            visible: !rows.is_empty(),
            rows,
            row_views: Vec::new(),
            list_box: list_box.clone(),
        };
        let mut model = model;
        model.sync_rows();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            WorldClockInput::Update(rows) => {
                self.visible = !rows.is_empty();
                self.rows = rows;
                self.sync_rows();
            }
        }
    }
}

impl WorldClock {
    fn sync_rows(&mut self) {
        if self.row_views.len() != self.rows.len() {
            while let Some(child) = self.list_box.first_child() {
                self.list_box.remove(&child);
            }
            self.row_views.clear();
            for _ in &self.rows {
                let view = WorldClockRowView::init(());
                self.list_box.append(view.as_ref());
                self.row_views.push(view);
            }
        }

        for (view, row) in self.row_views.iter().zip(&self.rows) {
            view.name.set_label(&row.name);
            view.day.set_label(row.day_label.unwrap_or(""));
            view.day.set_visible(row.day_label.is_some());
            view.time.set_label(&row.time);
            view.offset.set_label(&row.offset);
        }
    }
}
