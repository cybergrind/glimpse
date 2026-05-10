use relm4::{
    factory::{DynamicIndex, FactoryComponent, FactorySender},
    gtk::{self, prelude::*},
};

use super::strip::{Output, PagerAppearance, PagerItem};

pub struct Item {
    view: PagerItem,
}

pub struct Widgets {
    root: gtk::Box,
    label: gtk::Label,
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

    fn apply_view(&self, root: &gtk::Box, label: &gtk::Label) {
        set_class(
            root,
            "pager-dot",
            self.view.appearance == PagerAppearance::Dots,
        );
        set_class(
            root,
            "pager-num",
            self.view.appearance == PagerAppearance::Numbers,
        );
        set_class(root, "active", self.view.focused);
        set_class(root, "occupied", self.view.occupied && !self.view.focused);
        set_class(root, "urgent", self.view.urgent);
        label.set_visible(self.view.appearance == PagerAppearance::Numbers);
        label.set_label(&self.view.label);
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

        let label = gtk::Label::new(None);
        label.set_valign(gtk::Align::Center);
        label.set_halign(gtk::Align::Center);
        label.set_hexpand(true);
        label.set_vexpand(true);
        label.set_xalign(0.5);
        label.set_yalign(0.5);
        root.append(&label);

        let widgets = Widgets {
            root: root.clone(),
            label,
        };
        self.apply_view(&widgets.root, &widgets.label);
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
