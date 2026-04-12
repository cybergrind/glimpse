use glimpse::providers::battery::BatteryStatus;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct BatteryDetails {
    health: String,
    model_name: String,
    rate: String,
    show_rate: bool,
    charge_limit: String,
    show_charge_limit: bool,
}

#[derive(Debug)]
pub enum BatteryDetailsInput {
    Update(BatteryStatus),
}

#[relm4::component(pub)]
impl SimpleComponent for BatteryDetails {
    type Init = ();
    type Input = BatteryDetailsInput;
    type Output = ();

    view! {
        gtk::Box {
            add_css_class: "battery-details",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            gtk::Box {
                add_css_class: "detail-row",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Label {
                    add_css_class: "detail-key",
                    set_label: "Health",
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                },

                gtk::Label {
                    add_css_class: "detail-val",
                    #[watch]
                    set_label: &model.health,
                },
            },

            gtk::Box {
                add_css_class: "detail-row",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Label {
                    add_css_class: "detail-key",
                    set_label: "Model",
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                },

                gtk::Label {
                    add_css_class: "detail-val",
                    #[watch]
                    set_label: &model.model_name,
                },
            },

            gtk::Box {
                add_css_class: "detail-row",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                #[watch]
                set_visible: model.show_rate,

                gtk::Label {
                    add_css_class: "detail-key",
                    set_label: "Rate",
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                },

                gtk::Label {
                    add_css_class: "detail-val",
                    #[watch]
                    set_label: &model.rate,
                },
            },

            gtk::Box {
                add_css_class: "detail-row",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                #[watch]
                set_visible: model.show_charge_limit,

                gtk::Label {
                    add_css_class: "detail-key",
                    set_label: "Charge limit",
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                },

                gtk::Label {
                    add_css_class: "detail-val",
                    #[watch]
                    set_label: &model.charge_limit,
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = BatteryDetails {
            health: "\u{2014}".into(),
            model_name: "\u{2014}".into(),
            rate: String::new(),
            show_rate: false,
            charge_limit: String::new(),
            show_charge_limit: false,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BatteryDetailsInput::Update(status) => {
                self.health = format!("{:.0}%", status.capacity);
                self.model_name = if status.model.is_empty() {
                    "\u{2014}".into()
                } else {
                    status.model
                };

                self.show_rate = status.energy_rate > 0.0;
                self.rate = if self.show_rate {
                    format!("{:.1}W", status.energy_rate)
                } else {
                    String::new()
                };

                self.show_charge_limit = status.charge_threshold > 0;
                self.charge_limit = if self.show_charge_limit {
                    format!("{}%", status.charge_threshold)
                } else {
                    String::new()
                };
            }
        }
    }
}
