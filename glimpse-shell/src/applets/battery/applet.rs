use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        battery::{BatteryHandle, BatteryStatus, State},
        framework::ServiceCommand,
        power::{self, PowerHandle},
    },
};

use super::{
    format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    show_icon: bool,
    #[serde(alias = "label_format")]
    label_on_battery: String,
    #[serde(alias = "label_format_on_ac")]
    label_on_ac: String,
    #[serde(alias = "tooltip_format")]
    tooltip_on_battery: String,
    #[serde(alias = "tooltip_format_on_ac")]
    tooltip_on_ac: String,
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
            label_on_battery: String::new(),
            label_on_ac: String::new(),
            tooltip_on_battery: "{percentage}% {state}, {time_left}".into(),
            tooltip_on_ac: "{percentage}% {state}".into(),
            settings_command: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::battery::BatteryState;

    #[test]
    fn default_config_hides_label() {
        let config = Config::default();
        let status = BatteryStatus {
            present: true,
            on_battery: true,
            percentage: 73,
            state: BatteryState::Discharging,
            ..BatteryStatus::default()
        };
        let (label_template, _) = select_templates(&config, &status);

        assert_eq!(label_template, "");
        assert_eq!(format::label(label_template, &status), "");
    }
}

pub struct Applet {
    visible: bool,
    config: Config,
    tooltip: String,
    label: String,
    icon_name: String,
    service: BatteryHandle,
    power_service: PowerHandle,
    popover: Controller<Popover>,
    popover_open: bool,
    latest_status: BatteryStatus,
    latest_profiles: power::PowerProfiles,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: BatteryHandle,
    pub power_service: PowerHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    BatteryStateChanged(State),
    PowerStateChanged(power::State),
    Reconfigure(Config),
    TogglePopover,
    PopoverOpened,
    PopoverClosed,
    SetPowerProfile(String),
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
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = Popover::builder()
            .launch(PopoverInit {
                parent: root.clone(),
            })
            .forward(sender.input_sender(), |output| match output {
                PopoverOutput::Opened => Input::PopoverOpened,
                PopoverOutput::Closed => Input::PopoverClosed,
                PopoverOutput::SetProfile(profile) => Input::SetPowerProfile(profile),
            });

        let latest_status = init.service.snapshot().status;
        let latest_profiles = init.power_service.snapshot().profiles;
        let model = Applet {
            label: String::new(),
            tooltip: String::new(),
            visible: true,
            config: init.config,
            icon_name: "battery-missing-symbolic".into(),
            service: init.service,
            power_service: init.power_service,
            popover,
            popover_open: false,
            latest_status,
            latest_profiles,
            subscription_cancel: CancellationToken::new(),
        };

        let service = model.service.clone();
        let cancel = model.subscription_cancel.clone();
        let subscription_sender = sender.clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            subscription_sender.input(Input::BatteryStateChanged(sub.borrow().clone()));

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() {
                            break;
                        }

                        subscription_sender.input(Input::BatteryStateChanged(sub.borrow().clone()));
                    }
                }
            }
        });

        let service = model.power_service.clone();
        let cancel = model.subscription_cancel.clone();
        let subscription_sender = sender.clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            subscription_sender.input(Input::PowerStateChanged(sub.borrow().clone()));

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() {
                            break;
                        }

                        subscription_sender.input(Input::PowerStateChanged(sub.borrow().clone()));
                    }
                }
            }
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::BatteryStateChanged(state) => {
                let status = state.status;
                apply_status(self, &status);
                self.latest_status = status.clone();
                if self.popover_open {
                    self.popover.emit(PopoverInput::UpdateStatus(status));
                }
            }
            Input::PowerStateChanged(state) => {
                self.latest_profiles = state.profiles.clone();
                if self.popover_open {
                    self.popover
                        .emit(PopoverInput::UpdateProfiles(state.profiles));
                }
            }
            Input::Reconfigure(config) => {
                self.config = config;
                let snapshot = self.service.snapshot();
                apply_status(self, &snapshot.status);
                self.latest_status = snapshot.status;
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::PopoverOpened => {
                self.popover_open = true;
                self.sync_popover();
            }
            Input::PopoverClosed => {
                self.popover_open = false;
            }
            Input::SetPowerProfile(profile) => {
                let service = self.power_service.clone();
                relm4::spawn(async move {
                    if let Err(error) = service
                        .send(ServiceCommand::Command(power::Command::SetProfile(profile)))
                        .await
                    {
                        tracing::warn!(%error, "failed to set power profile");
                    }
                });
            }
        }
    }
}

impl Applet {
    fn sync_popover(&self) {
        self.popover
            .emit(PopoverInput::UpdateStatus(self.latest_status.clone()));
        self.popover
            .emit(PopoverInput::UpdateProfiles(self.latest_profiles.clone()));
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn apply_status(instance: &mut Applet, status: &BatteryStatus) {
    let (label_template, tooltip_template) = select_templates(&instance.config, status);

    instance.label = format::label(label_template, status);
    instance.tooltip = format::label(tooltip_template, status);
    instance.icon_name = status.icon_name.clone();
    instance.visible = status.present;
}

fn select_templates<'a>(config: &'a Config, status: &BatteryStatus) -> (&'a str, &'a str) {
    if status.on_battery {
        (&config.label_on_battery, &config.tooltip_on_battery)
    } else {
        (
            if config.label_on_ac.is_empty() {
                &config.label_on_battery
            } else {
                &config.label_on_ac
            },
            if config.tooltip_on_ac.is_empty() {
                &config.tooltip_on_battery
            } else {
                &config.tooltip_on_ac
            },
        )
    }
}
