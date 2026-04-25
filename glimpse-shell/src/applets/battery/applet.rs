use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;

use crate::{
    panels::applets::AppletConfig,
    services::battery::{BatteryHandle, BatteryState, BatteryStatus, State},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    show_icon: bool,
    label_format: String,
    label_format_on_ac: String,
    tooltip_format: String,
    tooltip_format_on_ac: String,
    settings_command: String,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid battery applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_icon: true,
            label_format: "".into(),
            label_format_on_ac: "".into(),
            tooltip_format: "{percentage}% {state}, {time_left}".into(),
            tooltip_format_on_ac: "{percentage}% {state}".into(),
            settings_command: String::new(),
        }
    }
}

pub struct Applet {
    visible: bool,
    config: Config,
    tooltip: String,
    label: String,
    icon_name: String,
    service: BatteryHandle,
}

#[derive(Debug)]
pub struct Init {
    pub service: BatteryHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    BatteryStateChanged(State),
    Reconfigure(Config),
    TogglePopover,
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            add_css_class: "battery",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            #[watch]
            set_visible: model.visible,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() {None} else {Some(&model.tooltip)},

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::TogglePopover);
                },
            },

            gtk::Image {
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                #[watch]
                set_icon_name: Some(&model.icon_name),
                #[watch]
                set_visible: model.config.show_icon,
            },

            gtk::Label {
                set_valign: gtk::Align::Center,
                #[watch]
                set_label: &model.label,
                #[watch]
                set_visible: !model.label.is_empty(),
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Applet {
            label: String::new(),
            tooltip: String::new(),
            visible: false,
            config: init.config,
            icon_name: "battery-missing-symbolic".into(),
            service: init.service,
        };

        let service = model.service.clone();
        let subscription_sender = sender.clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            subscription_sender.input(Input::BatteryStateChanged(sub.borrow().clone()));

            loop {
                if sub.changed().await.is_err() {
                    break;
                }

                subscription_sender.input(Input::BatteryStateChanged(sub.borrow().clone()));
            }
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::BatteryStateChanged(state) => {
                apply_status(self, &state.status);
            }
            Input::Reconfigure(config) => {
                self.config = config;
                let snapshot = self.service.snapshot();
                apply_status(self, &snapshot.status);
            }
            Input::TogglePopover => {}
        }
    }
}

fn apply_status(instance: &mut Applet, status: &BatteryStatus) {
    let (label_template, tooltip_template) = if status.on_battery {
        (
            &instance.config.label_format,
            &instance.config.tooltip_format,
        )
    } else {
        (
            &instance.config.label_format_on_ac,
            &instance.config.tooltip_format_on_ac,
        )
    };

    instance.label = format_label(label_template, status);
    instance.tooltip = format_label(tooltip_template, status);
    instance.icon_name = status.icon_name.clone();
    instance.visible = status.present;
}

fn format_label(template: &str, status: &BatteryStatus) -> String {
    if template.is_empty() {
        return String::new();
    }

    template
        .replace("{percentage}", &status.percentage.to_string())
        .replace("{state}", format_state(&status.state).as_ref())
        .replace("{time_left}", &format_time_left(status))
        .trim_end_matches([' ', ',', '-', '—'])
        .to_owned()
}

fn format_state(state: &BatteryState) -> &'static str {
    match state {
        BatteryState::Charging => "charging",
        BatteryState::Discharging => "discharging",
        BatteryState::Empty => "empty",
        BatteryState::FullyCharged => "fully charged",
        BatteryState::PendingCharge => "pending charge",
        BatteryState::PendingDischarge => "pending discharge",
        BatteryState::Unknown => "unknown",
    }
}

fn format_time_left(status: &BatteryStatus) -> String {
    let seconds = if status.on_battery {
        status.time_to_empty
    } else {
        status.time_to_full
    };

    if seconds <= 0 {
        return String::new();
    }

    let minutes = seconds / 60;
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;

    if hours > 0 {
        format!("{hours}h {remaining_minutes}m")
    } else {
        format!("{remaining_minutes}m")
    }
}
