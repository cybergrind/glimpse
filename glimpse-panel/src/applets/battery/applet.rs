use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, prelude::*},
};

use super::config::BatteryConfig;

pub struct Battery {
    config: BatteryConfig,
    icon_name: String,
    label: String,
    tooltip: String,
    visible: bool,
}

pub struct BatteryInit {
    pub config: BatteryConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum BatteryInput {
    Update(serde_json::Value),
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Battery {
    type Init = BatteryInit;
    type Input = BatteryInput;
    type Output = ();
    type CommandOutput = BatteryInput;

    view! {
        gtk::Box {
            set_spacing: 4,
            add_css_class: "battery",
            #[watch]
            set_visible: model.visible,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 16,
                #[watch]
                set_visible: model.config.show_icon,
            },

            gtk::Label {
                #[watch]
                set_label: &model.label,
                add_css_class: "battery-label",
                #[watch]
                set_visible: !model.label.is_empty(),
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Battery {
            config: init.config,
            icon_name: "battery-missing-symbolic".into(),
            label: String::new(),
            tooltip: String::new(),
            visible: false,
        };

        let client = init.client;
        sender.command(move |cmd_tx, shutdown| {
            shutdown
                .register(async move {
                    let mut sub = match client.subscribe("battery.status").await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("failed to subscribe to battery.status: {e}");
                            let _ = cmd_tx.send(BatteryInput::Unavailable);
                            return;
                        }
                    };
                    while let Some(event) = sub.next().await {
                        let _ = cmd_tx.send(BatteryInput::Update(event.data));
                    }
                    let _ = cmd_tx.send(BatteryInput::Unavailable);
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            BatteryInput::Update(data) => {
                let percentage = data["percentage"].as_u64().unwrap_or(0).min(100) as u8;
                let state = data["state"].as_str().unwrap_or("unknown");
                let icon_name = data["icon_name"].as_str().unwrap_or("battery-missing-symbolic");
                let present = data["present"].as_bool().unwrap_or(false);
                let on_battery = data["on_battery"].as_bool().unwrap_or(false);
                let energy_rate = data["energy_rate"].as_f64().unwrap_or(0.0);
                let capacity = data["capacity"].as_f64().unwrap_or(0.0);
                let time_to_empty = data["time_to_empty"].as_i64().unwrap_or(0);
                let time_to_full = data["time_to_full"].as_i64().unwrap_or(0);

                self.icon_name = icon_name.to_owned();
                self.visible = present;

                let vars = FormatVars {
                    percentage,
                    state,
                    energy_rate,
                    capacity,
                    time_to_empty,
                    time_to_full,
                };

                let (label_fmt, tooltip_fmt) = if on_battery {
                    (&self.config.label_on_battery, &self.config.tooltip_on_battery)
                } else {
                    (&self.config.label_on_ac, &self.config.tooltip_on_ac)
                };

                self.label = format_template(label_fmt, &vars);
                self.tooltip = format_template(tooltip_fmt, &vars);
            }
            BatteryInput::Unavailable => {
                self.visible = false;
            }
        }
    }
}

struct FormatVars<'a> {
    percentage: u8,
    state: &'a str,
    energy_rate: f64,
    capacity: f64,
    time_to_empty: i64,
    time_to_full: i64,
}

fn format_template(template: &str, vars: &FormatVars) -> String {
    if template.is_empty() {
        return String::new();
    }

    let time_left = match vars.state {
        "discharging" if vars.time_to_empty > 0 => {
            format!("{} remaining", format_duration(vars.time_to_empty))
        }
        "charging" if vars.time_to_full > 0 => {
            format!("{} until full", format_duration(vars.time_to_full))
        }
        "fully-charged" => "fully charged".into(),
        _ => String::new(),
    };

    let power = if vars.energy_rate > 0.0 {
        format!("{:.1}W", vars.energy_rate)
    } else {
        String::new()
    };

    template
        .replace("{percentage}", &vars.percentage.to_string())
        .replace("{state}", vars.state)
        .replace("{time_left}", &time_left)
        .replace("{power}", &power)
        .replace("{health}", &format!("{:.0}%", vars.capacity))
        .trim_end_matches([' ', ',', '-', '—'])
        .to_owned()
}

fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else {
        format!("{minutes}m")
    }
}
