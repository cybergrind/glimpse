use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use super::config::NetworkConfig;
use super::popover::{NetworkPopover, NetworkPopoverInit, NetworkPopoverInput};

pub struct Network {
    primary_icon: String,
    vpn_icon_visible: bool,
    tooltip: String,
    show_vpn_icon: bool,
    popover: Controller<NetworkPopover>,
}

pub struct NetworkInit {
    pub config: NetworkConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum NetworkMsg {
    StatusUpdate {
        icon: String,
        primary_connection: String,
        primary_type: String,
        speed: u32,
        metered: bool,
        wifi_enabled: bool,
        connectivity: String,
    },
    ConnectionsUpdate {
        has_vpn: bool,
    },
    WifiUpdate(serde_json::Value),
    DevicesUpdate(serde_json::Value),
    SavedVpnsUpdate(serde_json::Value),
    TogglePopover,
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Network {
    type Init = NetworkInit;
    type Input = NetworkMsg;
    type Output = ();
    type CommandOutput = NetworkMsg;

    view! {
        gtk::Box {
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "network",
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(NetworkMsg::TogglePopover);
                }
            },

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.primary_icon),
                set_pixel_size: 16,
            },

            gtk::Image {
                set_icon_name: Some("network-vpn-symbolic"),
                set_pixel_size: 16,
                #[watch]
                set_visible: model.vpn_icon_visible && model.show_vpn_icon,
            },
        }
    }

    fn init(
        init: Self::Init, root: Self::Root, sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = NetworkPopover::builder()
            .launch(NetworkPopoverInit {
                parent: root.clone(),
                client: init.client.clone(),
                settings_command: init.config.settings_command.clone(),
            })
            .detach();

        let model = Network {
            primary_icon: "network-offline-symbolic".into(),
            vpn_icon_visible: false,
            tooltip: String::new(),
            show_vpn_icon: init.config.show_vpn_icon,
            popover,
        };

        let client = init.client;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("network applet: subscribing");
                    let mut status_sub = match client.subscribe("network.status").await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("network: subscribe failed: {e}");
                            let _ = out.send(NetworkMsg::Unavailable);
                            return;
                        }
                    };
                    let mut connections_sub = client.subscribe("network.connections").await.ok();
                    let mut wifi_sub = client.subscribe("network.wifi").await.ok();
                    let mut devices_sub = client.subscribe("network.devices").await.ok();
                    let mut vpns_sub = client.subscribe("network.saved_vpns").await.ok();

                    loop {
                        tokio::select! {
                            Some(ev) = status_sub.next() => {
                                let icon = ev.data["icon"].as_str().unwrap_or("network-offline-symbolic").to_string();
                                let primary_connection = ev.data["primary_connection"].as_str().unwrap_or("").to_string();
                                let primary_type = ev.data["primary_type"].as_str().unwrap_or("").to_string();
                                let speed = ev.data["speed"].as_u64().unwrap_or(0) as u32;
                                let metered = ev.data["metered"].as_bool().unwrap_or(false);
                                let wifi_enabled = ev.data["wifi_enabled"].as_bool().unwrap_or(false);
                                let connectivity = ev.data["connectivity"].as_str().unwrap_or("unknown").to_string();
                                let _ = out.send(NetworkMsg::StatusUpdate {
                                    icon, primary_connection, primary_type, speed, metered, wifi_enabled, connectivity,
                                });
                            }
                            Some(ev) = async {
                                match &mut connections_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                let has_vpn = ev.data.as_array()
                                    .map(|arr| arr.iter().any(|c| {
                                        let ct = c["connection_type"].as_str().unwrap_or("");
                                        ct == "vpn" || ct == "wireguard"
                                    }))
                                    .unwrap_or(false);
                                let _ = out.send(NetworkMsg::ConnectionsUpdate { has_vpn });
                            }
                            Some(ev) = async {
                                match &mut wifi_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                let _ = out.send(NetworkMsg::WifiUpdate(ev.data));
                            }
                            Some(ev) = async {
                                match &mut devices_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                let _ = out.send(NetworkMsg::DevicesUpdate(ev.data));
                            }
                            Some(ev) = async {
                                match &mut vpns_sub {
                                    Some(s) => s.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                let _ = out.send(NetworkMsg::SavedVpnsUpdate(ev.data));
                            }
                            else => break,
                        }
                    }
                    let _ = out.send(NetworkMsg::Unavailable);
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
            NetworkMsg::StatusUpdate { icon, primary_connection, primary_type, speed, metered, wifi_enabled, connectivity } => {
                tracing::info!(
                    primary_connection, primary_type, speed, metered, wifi_enabled, connectivity,
                    "network applet: status update"
                );
                self.primary_icon = icon.clone();

                self.tooltip = if connectivity == "none" || primary_connection.is_empty() {
                    "Network offline".into()
                } else {
                    let base = match primary_type.as_str() {
                        "802-11-wireless" => {
                            if speed > 0 {
                                format!("{primary_connection} \u{b7} {speed} Mbps")
                            } else {
                                primary_connection.clone()
                            }
                        }
                        "802-3-ethernet" => {
                            if speed > 0 {
                                format!("Wired \u{b7} {speed} Mbps")
                            } else {
                                "Wired".into()
                            }
                        }
                        _ => primary_connection.clone(),
                    };
                    let mut parts = vec![base];
                    if metered { parts.push("Metered".into()); }
                    if self.vpn_icon_visible { parts.push("VPN".into()); }
                    parts.join(" \u{b7} ")
                };

                self.popover.emit(NetworkPopoverInput::UpdateStatus {
                    primary_connection, primary_type, speed, metered, wifi_enabled, connectivity, icon,
                });
            }
            NetworkMsg::ConnectionsUpdate { has_vpn } => {
                self.vpn_icon_visible = has_vpn;
            }
            NetworkMsg::WifiUpdate(data) => {
                self.popover.emit(NetworkPopoverInput::UpdateWifi(data));
            }
            NetworkMsg::DevicesUpdate(data) => {
                self.popover.emit(NetworkPopoverInput::UpdateDevices(data));
            }
            NetworkMsg::SavedVpnsUpdate(data) => {
                self.popover.emit(NetworkPopoverInput::UpdateSavedVpns(data));
            }
            NetworkMsg::TogglePopover => {
                self.popover.emit(NetworkPopoverInput::Toggle);
            }
            NetworkMsg::Unavailable => {
                tracing::warn!("network applet: daemon unavailable");
            }
        }
    }
}
