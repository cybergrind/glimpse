#![allow(unused_assignments)]

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use crate::applets::exec::{
    protocol::{EventKind, EventPayload, EventSource, MouseButton, StatusItem as StatusItemModel},
    renderer::apply_icon_to_image,
};

pub struct StatusItem {
    item: StatusItemModel,
    has_popover: bool,
    image: gtk::Image,
}

#[derive(Debug, Clone)]
pub struct Init {
    pub item: StatusItemModel,
    pub has_popover: bool,
}

#[derive(Debug)]
pub enum Input {
    Click(u32),
    Scroll(f64),
    Reconfigure {
        item: StatusItemModel,
        has_popover: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    TogglePopover,
    ContextMenu,
    Event(EventPayload),
    Activate(Option<EventPayload>),
}

#[relm4::component(pub)]
impl SimpleComponent for StatusItem {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        gtk::Box {
            add_css_class: "exec-status-item",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            set_valign: gtk::Align::Center,
            #[watch]
            set_tooltip_text: model.item.tooltip.as_deref(),

            add_controller = gtk::GestureClick {
                set_button: 0,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(Input::Click(gesture.current_button()));
                }
            },

            add_controller = gtk::EventControllerScroll {
                set_flags: gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
                connect_scroll[sender] => move |_, _, delta_y| {
                    sender.input(Input::Scroll(delta_y));
                    glib::Propagation::Proceed
                }
            },

            #[name = "image"]
            gtk::Image {
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                #[watch]
                set_visible: model.item.icon.is_some(),
            },

            gtk::Label {
                add_css_class: "exec-status-label",
                set_valign: gtk::Align::Center,
                set_single_line_mode: true,
                set_ellipsize: gtk::pango::EllipsizeMode::End,
                #[watch]
                set_label: model.item.label.as_deref().unwrap_or_default(),
                #[watch]
                set_visible: model.item.label.is_some(),
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = StatusItem {
            item: init.item,
            has_popover: init.has_popover,
            image: gtk::Image::new(),
        };
        let widgets = view_output!();
        let mut model = model;
        model.image = widgets.image.clone();
        model.apply_icon();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::Click(button) => {
                if button == 3 {
                    let _ = sender.output(Output::ContextMenu);
                    return;
                }

                let event = self.item.id.as_ref().map(|id| EventPayload {
                    id: id.clone(),
                    kind: EventKind::Click,
                    source: EventSource::Status,
                    button: Some(MouseButton::from_number(button)),
                    active: None,
                    value: None,
                    delta_y: None,
                });

                if self.has_popover && button == 1 {
                    let output = match event {
                        Some(event) => Output::Activate(Some(event)),
                        None => Output::TogglePopover,
                    };
                    let _ = sender.output(output);
                    return;
                }

                if let Some(event) = event {
                    let _ = sender.output(Output::Event(event));
                }
            }
            Input::Scroll(delta_y) => {
                if let Some(id) = &self.item.id {
                    let _ = sender.output(Output::Event(EventPayload {
                        id: id.clone(),
                        kind: EventKind::Scroll,
                        source: EventSource::Status,
                        button: None,
                        active: None,
                        value: None,
                        delta_y: Some(delta_y),
                    }));
                }
            }
            Input::Reconfigure { item, has_popover } => {
                self.item = item;
                self.has_popover = has_popover;
                self.apply_icon();
            }
        }
    }
}

impl StatusItem {
    fn apply_icon(&self) {
        match &self.item.icon {
            Some(icon) => apply_icon_to_image(&self.image, icon),
            None => self.image.clear(),
        }
    }
}
