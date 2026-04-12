use glimpse::network::{
    NetworkServiceHandle,
    protocol::{NetworkActiveAction, NetworkServiceCommand, NetworkServiceState},
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use super::config::NetworkConfig;
use super::popover::{
    NetworkPopover, NetworkPopoverInit, NetworkPopoverInput, NetworkPopoverOutput,
};

pub struct Network {
    primary_icon: String,
    vpn_icon_visible: bool,
    connecting: bool,
    tooltip: String,
    show_vpn_icon: bool,
    settings_command: String,
    scan_interval: u64,
    service: NetworkServiceHandle,
    popover: Controller<NetworkPopover>,
}

pub struct NetworkInit {
    pub config: NetworkConfig,
    pub service: NetworkServiceHandle,
}

#[derive(Debug, Clone)]
pub enum NetworkMsg {
    ServiceState(NetworkServiceState),
    PopoverOutput(NetworkPopoverOutput),
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
                set_icon_name: Some(if model.connecting {
                    connecting_icon_name(&model.primary_icon)
                } else {
                    &model.primary_icon
                }),
                set_pixel_size: 16,
                #[watch]
                set_css_classes: if model.connecting { &["net-connecting"] } else { &[] },
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
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = NetworkPopover::builder()
            .launch(NetworkPopoverInit {
                parent: root.clone(),
                show_settings_button: !init.config.settings_command.is_empty(),
            })
            .forward(sender.input_sender(), NetworkMsg::PopoverOutput);

        let model = Network {
            primary_icon: "network-offline-symbolic".into(),
            vpn_icon_visible: false,
            connecting: false,
            tooltip: String::new(),
            show_vpn_icon: init.config.show_vpn_icon,
            settings_command: init.config.settings_command,
            scan_interval: init.config.scan_interval,
            service: init.service.clone(),
            popover,
        };

        let service = init.service;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("network applet: subscribing to network service");
                    let mut state_rx = service.subscribe();
                    let _ = out.send(NetworkMsg::ServiceState(state_rx.borrow().clone()));

                    loop {
                        if state_rx.changed().await.is_err() {
                            break;
                        }
                        let _ = out.send(NetworkMsg::ServiceState(state_rx.borrow().clone()));
                    }

                    tracing::warn!("network applet: network service state channel closed");
                    let _ = out.send(NetworkMsg::Unavailable);
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

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            NetworkMsg::ServiceState(state) => {
                self.vpn_icon_visible = has_active_vpn(&state);
                self.connecting = is_connecting(&state);
                self.primary_icon = state.snapshot.status.icon.clone();
                self.tooltip = tooltip_text(&state, self.vpn_icon_visible, self.connecting);
                self.popover.emit(NetworkPopoverInput::UpdateState {
                    snapshot: state.snapshot,
                    active_action: state.active_action,
                    scanning: state.scanning,
                });
            }
            NetworkMsg::PopoverOutput(output) => self.handle_popover_output(output, sender),
            NetworkMsg::TogglePopover => {
                self.popover.emit(NetworkPopoverInput::Toggle);
            }
            NetworkMsg::Unavailable => {
                tracing::warn!("network applet: network service unavailable");
            }
        }
    }
}

impl Network {
    fn handle_popover_output(&self, output: NetworkPopoverOutput, sender: ComponentSender<Self>) {
        if should_close_popover_before_output(&output) {
            self.popover.emit(NetworkPopoverInput::Close);
        }

        match output {
            NetworkPopoverOutput::Opened => {
                self.send_command(
                    sender,
                    NetworkServiceCommand::StartScanning {
                        interval_secs: self.scan_interval,
                    },
                );
            }
            NetworkPopoverOutput::Closed => {
                self.send_command(sender, NetworkServiceCommand::StopScanning);
            }
            NetworkPopoverOutput::ToggleWifi(enabled) => {
                self.send_command(sender, NetworkServiceCommand::SetWifiEnabled(enabled));
            }
            NetworkPopoverOutput::ConnectWifi { ssid, path } => {
                self.send_command(sender, NetworkServiceCommand::ConnectWifi { ssid, path });
            }
            NetworkPopoverOutput::ConnectSaved { uuid } => {
                self.send_command(sender, NetworkServiceCommand::ConnectSaved { uuid });
            }
            NetworkPopoverOutput::Disconnect { uuid } => {
                self.send_command(sender, NetworkServiceCommand::Disconnect { uuid });
            }
            NetworkPopoverOutput::Forget { uuid } => {
                self.send_command(sender, NetworkServiceCommand::Forget { uuid });
            }
            NetworkPopoverOutput::OpenSettings => {
                spawn_settings_command(&self.settings_command);
            }
        }
    }

    fn send_command(&self, sender: ComponentSender<Self>, command: NetworkServiceCommand) {
        let service = self.service.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    if let Err(error) = service.send(command).await {
                        tracing::warn!(error = %error, "network applet: failed to send network service command");
                    }
                })
                .drop_on_shutdown()
        });
    }
}

fn has_active_vpn(state: &NetworkServiceState) -> bool {
    state.snapshot.connections.iter().any(|connection| {
        (connection.connection_type == "vpn" || connection.connection_type == "wireguard")
            && connection.state == "activated"
    }) || state.snapshot.saved_vpns.iter().any(|vpn| vpn.active)
}

fn is_connecting(state: &NetworkServiceState) -> bool {
    matches!(
        state.active_action,
        Some(NetworkActiveAction::ConnectWifi { .. })
            | Some(NetworkActiveAction::ConnectSaved { .. })
    ) || state.snapshot.connections.iter().any(|connection| {
        (connection.connection_type == "wifi" || connection.connection_type == "ethernet")
            && connection.state == "activating"
    })
}

fn should_close_popover_before_output(output: &NetworkPopoverOutput) -> bool {
    matches!(
        output,
        NetworkPopoverOutput::ConnectWifi { .. }
            | NetworkPopoverOutput::ConnectSaved { .. }
            | NetworkPopoverOutput::OpenSettings
    )
}

fn tooltip_text(state: &NetworkServiceState, has_vpn: bool, connecting: bool) -> String {
    let status = &state.snapshot.status;
    if connecting {
        return "Connecting…".into();
    }
    if status.connectivity == "none" || status.primary_connection.is_empty() {
        return "Network offline".into();
    }

    let base = match status.primary_type.as_str() {
        "wifi" => {
            if status.speed > 0 {
                format!("{} · {} Mbps", status.primary_connection, status.speed)
            } else {
                status.primary_connection.clone()
            }
        }
        "ethernet" => {
            if status.speed > 0 {
                format!("Wired · {} Mbps", status.speed)
            } else {
                "Wired".into()
            }
        }
        _ => status.primary_connection.clone(),
    };

    let mut parts = vec![base];
    if status.metered {
        parts.push("Metered".into());
    }
    if has_vpn {
        parts.push("VPN".into());
    }
    parts.join(" · ")
}

fn connecting_icon_name(primary_icon: &str) -> &str {
    if primary_icon == "network-wired-symbolic" {
        "network-wired-acquiring-symbolic"
    } else {
        "network-wireless-acquiring-symbolic"
    }
}

fn spawn_settings_command(command: &str) {
    if command.trim().is_empty() {
        return;
    }

    let command = command.to_owned();
    if let Ok(mut child) = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::network::provider::{
        NetworkConnection, NetworkSnapshot, NetworkStatus, SavedVpn,
    };

    #[test]
    fn tooltip_includes_speed_metered_and_vpn() {
        let state = NetworkServiceState {
            health: glimpse::network::protocol::NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                status: NetworkStatus {
                    connectivity: "full".into(),
                    primary_connection: "Home".into(),
                    primary_type: "wifi".into(),
                    speed: 585,
                    metered: true,
                    ..NetworkStatus::default()
                },
                saved_vpns: vec![SavedVpn {
                    active: true,
                    ..SavedVpn::default()
                }],
                ..NetworkSnapshot::default()
            },
            prompt: None,
            active_action: None,
            scanning: false,
        };

        assert_eq!(
            tooltip_text(&state, true, false),
            "Home · 585 Mbps · Metered · VPN"
        );
    }

    #[test]
    fn connecting_detects_pending_action_or_activating_connection() {
        let mut state = NetworkServiceState {
            health: glimpse::network::protocol::NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot::default(),
            prompt: None,
            active_action: Some(NetworkActiveAction::ConnectWifi {
                ssid: "Cafe".into(),
                path: "/ap/1".into(),
            }),
            scanning: false,
        };
        assert!(is_connecting(&state));

        state.active_action = None;
        state.snapshot.connections.push(NetworkConnection {
            connection_type: "wifi".into(),
            state: "activating".into(),
            ..NetworkConnection::default()
        });
        assert!(is_connecting(&state));
    }

    #[test]
    fn connect_and_settings_outputs_close_the_popover_first() {
        assert!(should_close_popover_before_output(
            &NetworkPopoverOutput::ConnectWifi {
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
            }
        ));
        assert!(should_close_popover_before_output(
            &NetworkPopoverOutput::ConnectSaved {
                uuid: "uuid-1".into(),
            }
        ));
        assert!(should_close_popover_before_output(
            &NetworkPopoverOutput::OpenSettings
        ));
        assert!(!should_close_popover_before_output(
            &NetworkPopoverOutput::Forget {
                uuid: "uuid-1".into(),
            }
        ));
    }
}
