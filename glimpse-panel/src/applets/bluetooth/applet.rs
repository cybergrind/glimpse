use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use super::config::BluetoothConfig;
use super::popover::{BtDevice, BluetoothPopover, BluetoothPopoverInit, BluetoothPopoverInput};

pub struct Bluetooth {
    icon_name: String,
    tooltip: String,
    popover: Controller<BluetoothPopover>,
}

pub struct BluetoothInit {
    pub config: BluetoothConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum BluetoothMsg {
    StatusUpdate { powered: bool, discovering: bool, connected_count: u32 },
    DevicesUpdate(Vec<BtDevice>),
    TogglePopover,
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Bluetooth {
    type Init = BluetoothInit;
    type Input = BluetoothMsg;
    type Output = ();
    type CommandOutput = BluetoothMsg;

    view! {
        gtk::Box {
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "bluetooth",
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(BluetoothMsg::TogglePopover);
                }
            },

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 16,
            },
        }
    }

    fn init(
        init: Self::Init, root: Self::Root, sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = BluetoothPopover::builder()
            .launch(BluetoothPopoverInit {
                parent: root.clone(),
                client: init.client.clone(),
                settings_command: init.config.settings_command,
                scan_interval: init.config.scan_interval,
            })
            .detach();

        let model = Bluetooth {
            icon_name: "bluetooth-active-symbolic".into(),
            tooltip: "Bluetooth".into(),
            popover,
        };

        let client = init.client;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("bluetooth applet: subscribing");
                    let mut status_sub = match client.subscribe("bluetooth.status").await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("bluetooth: subscribe failed: {e}");
                            let _ = out.send(BluetoothMsg::Unavailable);
                            return;
                        }
                    };
                    let mut devices_sub = client.subscribe("bluetooth.devices").await.ok();

                    loop {
                        tokio::select! {
                            Some(ev) = status_sub.next() => {
                                let powered = ev.data["powered"].as_bool().unwrap_or(false);
                                let discovering = ev.data["discovering"].as_bool().unwrap_or(false);
                                let connected_count = ev.data["connected_count"].as_u64().unwrap_or(0) as u32;
                                let _ = out.send(BluetoothMsg::StatusUpdate { powered, discovering, connected_count });
                            }
                            Some(ev) = async {
                                match &mut devices_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                if let Ok(devices) = serde_json::from_value(ev.data) {
                                    let _ = out.send(BluetoothMsg::DevicesUpdate(devices));
                                }
                            }
                            else => break,
                        }
                    }
                    let _ = out.send(BluetoothMsg::Unavailable);
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(&mut self, msg: Self::CommandOutput, sender: ComponentSender<Self>, root: &Self::Root) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            BluetoothMsg::StatusUpdate { powered, discovering, connected_count } => {
                tracing::info!(powered, discovering, connected_count, "bluetooth applet: status update");
                self.icon_name = if !powered {
                    "bluetooth-disabled-symbolic"
                } else {
                    "bluetooth-active-symbolic"
                }.into();

                self.tooltip = if !powered {
                    "Bluetooth off".into()
                } else if connected_count > 0 {
                    format!("{connected_count} device{} connected", if connected_count > 1 { "s" } else { "" })
                } else {
                    "Bluetooth".into()
                };

                self.popover.emit(BluetoothPopoverInput::UpdateStatus { powered, discovering });
            }
            BluetoothMsg::DevicesUpdate(devices) => {
                self.popover.emit(BluetoothPopoverInput::UpdateDevices(devices));
            }
            BluetoothMsg::TogglePopover => {
                self.popover.emit(BluetoothPopoverInput::Toggle);
            }
            BluetoothMsg::Unavailable => {
                tracing::warn!("bluetooth applet: daemon unavailable");
            }
        }
    }
}
