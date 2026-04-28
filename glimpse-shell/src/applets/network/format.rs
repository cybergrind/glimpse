use crate::services::network::{
    NetworkActiveAction, NetworkServiceHealth, NetworkSnapshot, State, WifiAccessPoint,
};

pub const DEFAULT_LABEL_FORMAT: &str = "";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{state}";

pub fn label(template: &str, state: &State) -> String {
    render(template, state)
}

pub fn tooltip(template: &str, state: &State) -> String {
    if template.is_empty() {
        return String::new();
    }

    render(template, state)
}

pub fn state_text(state: &State) -> String {
    match &state.health {
        NetworkServiceHealth::Starting => return "starting".into(),
        NetworkServiceHealth::Reconnecting { .. } => return "reconnecting".into(),
        NetworkServiceHealth::Degraded { message } => return message.clone(),
        NetworkServiceHealth::Ready => {}
    }

    if let Some(action) = active_action_text(state.active_action.as_ref()) {
        return action.into();
    }

    let status = &state.snapshot.status;
    if !status.enabled {
        "off".into()
    } else if !status.wifi_hw_enabled {
        "wifi unavailable".into()
    } else if !status.wifi_enabled {
        "wifi off".into()
    } else if !status.primary_connection.is_empty() {
        "connected".into()
    } else if state.scanning {
        "scanning".into()
    } else {
        "disconnected".into()
    }
}

pub fn wifi_icon(strength: u8) -> &'static str {
    match strength {
        75..=100 => "network-wireless-signal-excellent-symbolic",
        50..=74 => "network-wireless-signal-good-symbolic",
        25..=49 => "network-wireless-signal-ok-symbolic",
        1..=24 => "network-wireless-signal-weak-symbolic",
        _ => "network-wireless-signal-none-symbolic",
    }
}

pub fn hero_subtitle(state: &State) -> String {
    match &state.health {
        NetworkServiceHealth::Starting => return "Starting".into(),
        NetworkServiceHealth::Reconnecting { .. } => return "Reconnecting".into(),
        NetworkServiceHealth::Degraded { message } => return message.clone(),
        NetworkServiceHealth::Ready => {}
    }

    if let Some(prompt) = &state.prompt {
        return format!("Password required for {}", prompt.ssid);
    }

    if let Some(activity) = active_action_title(state) {
        return activity;
    }

    let status = &state.snapshot.status;
    if !status.enabled {
        "Off".into()
    } else if !status.wifi_hw_enabled {
        "Wi-Fi unavailable".into()
    } else if !status.wifi_enabled {
        "Wi-Fi off".into()
    } else if state.scanning {
        "Scanning".into()
    } else if !status.primary_connection.is_empty() {
        format!("Connected to {}", status.primary_connection)
    } else {
        "Not connected".into()
    }
}

pub fn wifi_status(access_point: &WifiAccessPoint) -> String {
    format!("{}%", access_point.strength)
}

fn render(template: &str, state: &State) -> String {
    if template.is_empty() {
        return String::new();
    }

    let snapshot = &state.snapshot;
    template
        .replace("{state}", &state_text(state))
        .replace("{network}", &snapshot.status.primary_connection)
        .replace("{type}", &snapshot.status.primary_type)
        .replace("{wifi}", &snapshot.wifi_access_points.len().to_string())
        .replace(
            "{access_points}",
            &snapshot.wifi_access_points.len().to_string(),
        )
        .replace("{connections}", &snapshot.connections.len().to_string())
        .replace("{vpns}", &snapshot.saved_vpns.len().to_string())
        .replace("{speed}", &speed_text(snapshot.status.speed))
        .trim()
        .to_owned()
}

fn active_action_text(action: Option<&NetworkActiveAction>) -> Option<&'static str> {
    match action? {
        NetworkActiveAction::SetWifiEnabled(true) => Some("turning wifi on"),
        NetworkActiveAction::SetWifiEnabled(false) => Some("turning wifi off"),
        NetworkActiveAction::ConnectWifi { .. } | NetworkActiveAction::ConnectSaved { .. } => {
            Some("connecting")
        }
        NetworkActiveAction::Disconnect { .. } => Some("disconnecting"),
        NetworkActiveAction::Forget { .. } => Some("forgetting"),
    }
}

fn active_action_title(state: &State) -> Option<String> {
    match state.active_action.as_ref()? {
        NetworkActiveAction::SetWifiEnabled(true) => Some("Turning Wi-Fi on".into()),
        NetworkActiveAction::SetWifiEnabled(false) => Some("Turning Wi-Fi off".into()),
        NetworkActiveAction::ConnectWifi { ssid, .. } => Some(format!("Connecting to {ssid}")),
        NetworkActiveAction::ConnectSaved { uuid } => Some(format!(
            "Connecting to {}",
            connection_name(&state.snapshot, uuid)
        )),
        NetworkActiveAction::Disconnect { uuid } => Some(format!(
            "Disconnecting {}",
            connection_name(&state.snapshot, uuid)
        )),
        NetworkActiveAction::Forget { uuid } => Some(format!(
            "Forgetting {}",
            connection_name(&state.snapshot, uuid)
        )),
    }
}

fn connection_name(snapshot: &NetworkSnapshot, uuid: &str) -> String {
    snapshot
        .connections
        .iter()
        .find(|connection| connection.uuid == uuid)
        .map(|connection| connection.id.clone())
        .or_else(|| {
            snapshot
                .saved_vpns
                .iter()
                .find(|vpn| vpn.uuid == uuid)
                .map(|vpn| vpn.id.clone())
        })
        .or_else(|| {
            snapshot
                .wifi_access_points
                .iter()
                .find(|access_point| access_point.uuid.as_deref() == Some(uuid))
                .map(|access_point| access_point.ssid.clone())
        })
        .unwrap_or_else(|| uuid.to_owned())
}

fn speed_text(speed_mbps: u32) -> String {
    if speed_mbps == 0 {
        return String::new();
    }

    format!("{speed_mbps} Mbps")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::network::{NetworkStatus, State};

    #[test]
    fn formats_state_and_connection_placeholders() {
        let state = State {
            snapshot: NetworkSnapshot {
                status: NetworkStatus {
                    enabled: true,
                    wifi_enabled: true,
                    wifi_hw_enabled: true,
                    primary_connection: "Home".into(),
                    primary_type: "wifi".into(),
                    speed: 866,
                    ..NetworkStatus::default()
                },
                wifi_access_points: vec![WifiAccessPoint::default()],
                ..NetworkSnapshot::default()
            },
            health: NetworkServiceHealth::Ready,
            ..State::default()
        };

        assert_eq!(label("{state}", &state), "connected");
        assert_eq!(
            tooltip("{network} {type} {wifi} {speed}", &state),
            "Home wifi 1 866 Mbps"
        );
    }

    #[test]
    fn wifi_icon_tracks_strength() {
        assert_eq!(wifi_icon(0), "network-wireless-signal-none-symbolic");
        assert_eq!(wifi_icon(30), "network-wireless-signal-ok-symbolic");
        assert_eq!(wifi_icon(80), "network-wireless-signal-excellent-symbolic");
    }
}
