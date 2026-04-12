use relm4::{
    factory::{DynamicIndex, FactoryComponent, FactorySender},
    gtk::{self, prelude::*},
};

use super::indicator_strip::{PagerIndicatorStripOutput, PagerIndicatorView};
use crate::applets::pager::config::PagerStyle;

pub struct PagerIndicatorItem {
    view: PagerIndicatorView,
    style: PagerStyle,
}

pub struct PagerIndicatorItemWidgets {
    root: gtk::Box,
    label: gtk::Label,
}

#[derive(Debug, Clone)]
pub struct PagerIndicatorItemInit {
    pub style: PagerStyle,
    pub view: PagerIndicatorView,
}

#[derive(Debug)]
pub enum PagerIndicatorItemInput {
    Update(PagerIndicatorView),
}

impl PagerIndicatorItem {
    pub fn key(&self) -> u32 {
        self.view.index
    }

    fn apply_view(&self, root: &gtk::Box, label: &gtk::Label) {
        match self.style {
            PagerStyle::Pills => {
                label.set_visible(false);
            }
            PagerStyle::Numbered => {
                label.set_visible(true);
                label.set_label(&self.view.index.to_string());
            }
        }

        set_class(root, "active", self.view.is_focused);
        set_class(
            root,
            "occupied",
            self.view.occupied && !self.view.is_focused,
        );
        set_class(root, "urgent", self.view.is_urgent);
    }
}

impl FactoryComponent for PagerIndicatorItem {
    type Init = PagerIndicatorItemInit;
    type Input = PagerIndicatorItemInput;
    type Output = PagerIndicatorStripOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = PagerIndicatorItemWidgets;
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self {
            view: init.view,
            style: init.style,
        }
    }

    fn init_root(&self) -> Self::Root {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.set_valign(gtk::Align::Center);
        root
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &gtk::Widget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let label = gtk::Label::new(None);
        root.append(&label);

        match self.style {
            PagerStyle::Pills => root.add_css_class("pager-dot"),
            PagerStyle::Numbered => root.add_css_class("pager-num"),
        }

        let click = gtk::GestureClick::new();
        click.set_button(1);
        let output = sender.output_sender().clone();
        let index = self.view.index;
        click.connect_pressed(move |_, _, _, _| {
            let _ = output.send(PagerIndicatorStripOutput::Click(index));
        });
        root.add_controller(click);

        let widgets = PagerIndicatorItemWidgets {
            root: root.clone(),
            label,
        };
        self.apply_view(&widgets.root, &widgets.label);

        widgets
    }

    fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        let PagerIndicatorItemInput::Update(view) = msg;
        self.view = view;
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: FactorySender<Self>) {
        self.apply_view(&widgets.root, &widgets.label);
    }
}

fn set_class(widget: &gtk::Box, class: &str, active: bool) {
    if active {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}
