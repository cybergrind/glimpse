use glimpse::providers::battery::BatteryStatus;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct BatteryDetails {
    health_val: gtk::Label,
    model_val: gtk::Label,
    rate_val: gtk::Label,
    charge_limit_row: gtk::Box,
    charge_limit_val: gtk::Label,
}

#[derive(Debug)]
pub enum BatteryDetailsInput {
    Update(BatteryStatus),
}

impl SimpleComponent for BatteryDetails {
    type Init = ();
    type Input = BatteryDetailsInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.add_css_class("battery-details");

        let health_val = build_detail_row(&root, "Health");
        let model_val = build_detail_row(&root, "Model");
        let rate_val = build_detail_row(&root, "Rate");

        let charge_limit_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        charge_limit_row.add_css_class("detail-row");

        let cl_key = gtk::Label::new(Some("Charge limit"));
        cl_key.set_hexpand(true);
        cl_key.set_halign(gtk::Align::Start);
        cl_key.add_css_class("detail-key");
        charge_limit_row.append(&cl_key);

        let charge_limit_val = gtk::Label::new(Some("—"));
        charge_limit_val.add_css_class("detail-val");
        charge_limit_row.append(&charge_limit_val);
        charge_limit_row.set_visible(false);
        root.append(&charge_limit_row);

        let model = BatteryDetails {
            health_val,
            model_val,
            rate_val,
            charge_limit_row,
            charge_limit_val,
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let BatteryDetailsInput::Update(status) = msg;

        self.health_val
            .set_label(&format!("{:.0}%", status.capacity));
        self.model_val.set_label(if status.model.is_empty() {
            "—"
        } else {
            &status.model
        });

        if status.energy_rate > 0.0 {
            self.rate_val
                .set_label(&format!("{:.1}W", status.energy_rate));
            self.rate_val.parent().map(|p| p.set_visible(true));
        } else {
            self.rate_val.parent().map(|p| p.set_visible(false));
        }

        if status.charge_threshold > 0 {
            self.charge_limit_row.set_visible(true);
            self.charge_limit_val
                .set_label(&format!("{}%", status.charge_threshold));
        } else {
            self.charge_limit_row.set_visible(false);
        }
    }
}

fn build_detail_row(parent: &gtk::Box, key: &str) -> gtk::Label {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("detail-row");

    let key_label = gtk::Label::new(Some(key));
    key_label.set_hexpand(true);
    key_label.set_halign(gtk::Align::Start);
    key_label.add_css_class("detail-key");
    row.append(&key_label);

    let val_label = gtk::Label::new(Some("—"));
    val_label.add_css_class("detail-val");
    row.append(&val_label);

    parent.append(&row);
    val_label
}
