use relm4::{
    factory::{DynamicIndex, FactoryComponent, FactorySender},
    gtk::{self, prelude::*},
};

use super::strip::{Output, PagerItem};

pub struct Item {
    view: PagerItem,
}

pub struct Widgets {
    root: gtk::Box,
}

#[derive(Debug, Clone)]
pub struct Init {
    pub view: PagerItem,
}

#[derive(Debug)]
pub enum Input {
    Update(PagerItem),
    Clicked,
}

impl Item {
    pub fn key(&self) -> usize {
        self.view.id
    }

    fn apply_view(&self, root: &gtk::Box) {
        set_class(root, "active", self.view.focused);
        set_class(root, "occupied", self.view.occupied && !self.view.focused);
        set_class(root, "urgent", self.view.urgent);
    }
}

impl FactoryComponent for Item {
    type Init = Init;
    type Input = Input;
    type Output = Output;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = Widgets;
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { view: init.view }
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
        let click = gtk::GestureClick::new();
        click.set_button(1);
        let input = sender.input_sender().clone();
        click.connect_pressed(move |_, _, _, _| {
            let _ = input.send(Input::Clicked);
        });
        root.add_controller(click);

        let widgets = Widgets { root: root.clone() };
        widgets.root.add_css_class("pager-dot");
        self.apply_view(&widgets.root);
        widgets
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            Input::Update(view) => self.view = view,
            Input::Clicked => {
                let _ = sender.output(Output::Activate(self.view.target));
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: FactorySender<Self>) {
        self.apply_view(&widgets.root);
    }
}

fn set_class(widget: &gtk::Box, class: &str, active: bool) {
    if active {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}
