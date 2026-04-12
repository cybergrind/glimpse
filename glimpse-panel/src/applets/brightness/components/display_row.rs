#![allow(unused_assignments)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use glimpse::providers::brightness::BrightnessDisplay;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

pub struct BrightnessDisplayRow {
    display: BrightnessDisplay,
    scale: gtk::Scale,
}

#[derive(Debug)]
pub enum BrightnessDisplayRowInput {
    Update(BrightnessDisplay),
    RequestedPercent(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrightnessDisplayRowOutput {
    SetPercent { display_id: String, percent: u8 },
}

#[relm4::component(pub)]
impl SimpleComponent for BrightnessDisplayRow {
    type Init = BrightnessDisplay;
    type Input = BrightnessDisplayRowInput;
    type Output = BrightnessDisplayRowOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 4,
            add_css_class: "brightness-row",

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Label {
                    #[watch]
                    set_label: &model.display.name,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_max_width_chars: 24,
                    set_halign: gtk::Align::Start,
                    add_css_class: "brightness-row-name",
                },
            },

            #[name(scale)]
            gtk::Scale {
                set_orientation: gtk::Orientation::Horizontal,
                set_draw_value: false,
                set_range: (0.0, 100.0),
                set_increments: (1.0, 5.0),
                add_css_class: "brightness-row-scale",
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = BrightnessDisplayRow {
            display: init,
            scale: gtk::Scale::new(gtk::Orientation::Horizontal, None::<&gtk::Adjustment>),
        };
        let widgets = view_output!();
        connect_throttled_scale(&widgets.scale, sender.clone());
        let mut model = model;
        model.scale = widgets.scale.clone();
        model.scale.set_value(f64::from(model.display.percentage));
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            BrightnessDisplayRowInput::Update(display) => {
                self.display = display;
                if !is_dragging(&self.scale) {
                    self.scale.set_value(f64::from(self.display.percentage));
                }
            }
            BrightnessDisplayRowInput::RequestedPercent(percent) => {
                self.display.percentage = percent;
                let _ = sender.output(BrightnessDisplayRowOutput::SetPercent {
                    display_id: self.display.id.clone(),
                    percent,
                });
            }
        }
    }
}

fn is_dragging(scale: &gtk::Scale) -> bool {
    scale.state_flags().contains(gtk::StateFlags::ACTIVE)
}

fn connect_throttled_scale(scale: &gtk::Scale, sender: ComponentSender<BrightnessDisplayRow>) {
    let last_sent = Rc::new(Cell::new(Instant::now()));
    let pending = Rc::new(Cell::new(false));

    scale.connect_change_value(move |scale, _, value| {
        let percent = value.round().clamp(0.0, 100.0) as u8;
        let now = Instant::now();

        if now.duration_since(last_sent.get()).as_millis() >= 150 {
            last_sent.set(now);
            pending.set(false);
            sender.input(BrightnessDisplayRowInput::RequestedPercent(percent));
        } else if !pending.get() {
            pending.set(true);
            let last_sent = last_sent.clone();
            let pending = pending.clone();
            let scale = scale.clone();
            let sender = sender.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
                if pending.get() {
                    pending.set(false);
                    last_sent.set(Instant::now());
                    sender.input(BrightnessDisplayRowInput::RequestedPercent(
                        scale.value().round().clamp(0.0, 100.0) as u8,
                    ));
                }
            });
        }

        glib::Propagation::Proceed
    });
}
