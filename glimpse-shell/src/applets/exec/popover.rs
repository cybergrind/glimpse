#![allow(unused_assignments)]

use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::components::{
    animated_popover::AnimatedPopover, popover_scroll, popover_shell::PopoverShell,
};

use super::{
    protocol::{EventPayload, TreeNode},
    renderer::RenderCatalog,
};

pub struct Popover {
    animation: AnimatedPopover,
    root_node: Option<TreeNode>,
    content_box: gtk::Box,
}

pub struct Init {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum Input {
    Toggle,
    Close,
    SetRoot(Option<TreeNode>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Opened,
    Closed,
    Event(EventPayload),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        root = gtk::Popover {
            add_css_class: "exec-popover",
            add_css_class: "popover-size-medium",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[name = "scroller"]
                    gtk::ScrolledWindow {
                        set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                        set_vexpand: false,
                        set_propagate_natural_height: true,

                        #[name = "content_box"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 0,
                        },
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        popover_scroll::install_half_monitor_limit(&widgets.root, &widgets.scroller, &init.parent);

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(Output::Opened);
        });

        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(Output::Closed);
        });

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            root_node: None,
            content_box: widgets.content_box.clone(),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::Toggle => self.animation.toggle(),
            Input::Close => self.animation.close(),
            Input::SetRoot(root) => {
                self.root_node = root;
                self.rebuild(&sender);
            }
        }
    }
}

impl Popover {
    fn rebuild(&self, sender: &ComponentSender<Self>) {
        while let Some(child) = self.content_box.first_child() {
            self.content_box.remove(&child);
        }

        let Some(root) = &self.root_node else {
            return;
        };

        let output_sender = sender.output_sender().clone();
        let renderer = RenderCatalog::new(Rc::new(move |event| {
            output_sender.emit(Output::Event(event));
        }));

        match renderer.render(root) {
            Ok(widget) => self.content_box.append(&widget),
            Err(error) => tracing::warn!(%error, "exec popover render failed"),
        }
    }
}
