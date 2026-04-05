use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

pub struct BatteryPopover {
    popover: gtk::Popover,
    client: Arc<Client>,
    // Status section.
    status_icon: gtk::Image,
    status_pct: gtk::Label,
    progress: gtk::ProgressBar,
    status_text: gtk::Label,
    // Details.
    health_val: gtk::Label,
    model_val: gtk::Label,
    rate_val: gtk::Label,
    // Charge limit.
    charge_limit_row: gtk::Box,
    charge_limit_val: gtk::Label,
    // Power profile.
    profile_box: gtk::Box,
}

pub struct BatteryPopoverInit {
    pub parent: gtk::Box,
    pub client: Arc<Client>,
    pub settings_command: String,
}

#[derive(Debug)]
pub enum BatteryPopoverInput {
    Toggle,
    UpdateStatus(serde_json::Value),
    UpdateProfiles(serde_json::Value),
}

fn spawn_call(client: &Arc<Client>, method: &'static str, params: serde_json::Value) {
    let c = client.clone();
    glib::spawn_future_local(async move { let _ = c.call(method, params).await; });
}

impl SimpleComponent for BatteryPopover {
    type Init = BatteryPopoverInit;
    type Input = BatteryPopoverInput;
    type Output = ();
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root { gtk::Popover::new() }

    fn init(
        init: Self::Init, root: Self::Root, _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("battery-popover");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        // === Battery status ===
        let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        status_row.add_css_class("battery-status-row");

        let status_icon = gtk::Image::from_icon_name("battery-missing-symbolic");
        status_icon.set_pixel_size(32);
        status_row.append(&status_icon);

        let status_pct = gtk::Label::new(Some("—"));
        status_pct.add_css_class("battery-pct");
        status_row.append(&status_pct);

        vbox.append(&status_row);

        let progress = gtk::ProgressBar::new();
        progress.add_css_class("battery-progress");
        vbox.append(&progress);

        let status_text = gtk::Label::new(None);
        status_text.set_halign(gtk::Align::Start);
        status_text.add_css_class("battery-state-text");
        vbox.append(&status_text);

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Details ===
        let health_val = build_detail_row(&vbox, "Health");
        let model_val = build_detail_row(&vbox, "Model");
        let rate_val = build_detail_row(&vbox, "Rate");

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Charge limit ===
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
        vbox.append(&charge_limit_row);

        // === Power profile ===
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let profile_label = gtk::Label::new(Some("Power profile"));
        profile_label.set_halign(gtk::Align::Start);
        profile_label.add_css_class("detail-key");
        vbox.append(&profile_label);

        let profile_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        profile_box.add_css_class("profile-list");
        vbox.append(&profile_box);

        // === Settings ===
        if !init.settings_command.is_empty() {
            vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            let cmd = init.settings_command;
            let lbl = gtk::Label::new(Some("Power Settings"));
            lbl.set_halign(gtk::Align::Start);
            let btn = gtk::Button::new();
            btn.set_child(Some(&lbl));
            btn.add_css_class("flat");
            btn.add_css_class("settings-btn");
            btn.connect_clicked(move |_| {
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if let Some((&prog, args)) = parts.split_first() {
                    let _ = std::process::Command::new(prog).args(args).spawn();
                }
            });
            vbox.append(&btn);
        }

        root.set_child(Some(&vbox));

        let model = BatteryPopover {
            popover: root.clone(), client: init.client,
            status_icon, status_pct, progress, status_text,
            health_val, model_val, rate_val,
            charge_limit_row, charge_limit_val,
            profile_box,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BatteryPopoverInput::Toggle => {
                if self.popover.is_visible() { self.popover.popdown(); }
                else { self.popover.popup(); }
            }
            BatteryPopoverInput::UpdateStatus(data) => {
                let pct = data["percentage"].as_u64().unwrap_or(0).min(100) as u8;
                let state = data["state"].as_str().unwrap_or("unknown");
                let icon = data["icon_name"].as_str().unwrap_or("battery-missing-symbolic");
                let model_name = data["model"].as_str().unwrap_or("");
                let energy_rate = data["energy_rate"].as_f64().unwrap_or(0.0);
                let capacity = data["capacity"].as_f64().unwrap_or(0.0);
                let tte = data["time_to_empty"].as_i64().unwrap_or(0);
                let ttf = data["time_to_full"].as_i64().unwrap_or(0);
                let threshold = data["charge_threshold"].as_u64().unwrap_or(0) as u32;

                self.status_icon.set_icon_name(Some(icon));
                self.status_pct.set_label(&format!("{pct}%"));
                self.progress.set_fraction(pct as f64 / 100.0);

                let state_text = match state {
                    "discharging" if tte > 0 => format!("Discharging — {} remaining", format_duration(tte)),
                    "discharging" => "Discharging".into(),
                    "charging" if ttf > 0 => format!("Charging — {} until full", format_duration(ttf)),
                    "charging" => "Charging".into(),
                    "fully-charged" => "Fully charged".into(),
                    "pending-charge" => "Plugged in, not charging".into(),
                    "pending-discharge" => "Plugged in".into(),
                    _ => state.to_owned(),
                };
                self.status_text.set_label(&state_text);

                self.health_val.set_label(&format!("{capacity:.0}%"));
                self.model_val.set_label(if model_name.is_empty() { "—" } else { model_name });

                if energy_rate > 0.0 {
                    self.rate_val.set_label(&format!("{energy_rate:.1}W"));
                    self.rate_val.parent().map(|p| p.set_visible(true));
                } else {
                    self.rate_val.parent().map(|p| p.set_visible(false));
                }

                if threshold > 0 {
                    self.charge_limit_row.set_visible(true);
                    self.charge_limit_val.set_label(&format!("{threshold}%"));
                } else {
                    self.charge_limit_row.set_visible(false);
                }
            }
            BatteryPopoverInput::UpdateProfiles(data) => {
                let active = data["active"].as_str().unwrap_or("");
                let available = data["available"].as_array();

                while let Some(child) = self.profile_box.first_child() {
                    self.profile_box.remove(&child);
                }

                if let Some(profiles) = available {
                    for p in profiles {
                        let name = p.as_str().unwrap_or("");
                        if name.is_empty() { continue; }

                        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                        row.add_css_class("profile-row");

                        let icon_name = match name {
                            "power-saver" => "power-profile-power-saver-symbolic",
                            "balanced" => "power-profile-balanced-symbolic",
                            "performance" => "power-profile-performance-symbolic",
                            _ => "power-profile-balanced-symbolic",
                        };
                        let icon = gtk::Image::from_icon_name(icon_name);
                        icon.set_pixel_size(16);
                        icon.add_css_class("profile-icon");
                        row.append(&icon);

                        let display_name = match name {
                            "power-saver" => "Power Saver",
                            "balanced" => "Balanced",
                            "performance" => "Performance",
                            _ => name,
                        };
                        let label = gtk::Label::new(Some(display_name));
                        label.set_hexpand(true);
                        label.set_halign(gtk::Align::Start);
                        row.append(&label);

                        if name == active {
                            let check = gtk::Image::from_icon_name("object-select-symbolic");
                            check.set_pixel_size(14);
                            check.add_css_class("profile-check");
                            row.append(&check);
                        }

                        let btn = gtk::Button::new();
                        btn.set_child(Some(&row));
                        btn.add_css_class("flat");
                        btn.add_css_class("profile-btn");

                        let c = self.client.clone();
                        let profile = name.to_owned();
                        btn.connect_clicked(move |_| {
                            spawn_call(&c, "power.set_profile", serde_json::json!(profile));
                        });

                        self.profile_box.append(&btn);
                    }
                }
            }
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

fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 { format!("{hours}h {minutes:02}m") }
    else { format!("{minutes}m") }
}
