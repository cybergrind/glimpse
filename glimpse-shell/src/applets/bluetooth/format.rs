use crate::services::bluetooth::{
    BluetoothActiveAction, BluetoothPrompt, BluetoothPromptKind, BluetoothServiceHealth,
    BluetoothSnapshot, State,
};

pub const DEFAULT_LABEL_FORMAT: &str = "";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{devices} connected devices";

pub fn label(template: &str, state: &State) -> String {
    render(template, state)
}

pub fn tooltip(template: &str, state: &State) -> String {
    if template.is_empty() {
        return String::new();
    }

    render(template, state)
}

fn render(template: &str, state: &State) -> String {
    if template.is_empty() {
        return String::new();
    }

    template
        .replace(
            "{devices}",
            &state.snapshot.status.connected_count.to_string(),
        )
        .replace("{state}", &state_text(state))
        .trim()
        .to_owned()
}

pub fn state_text(state: &State) -> String {
    match &state.health {
        BluetoothServiceHealth::Starting => return "starting".into(),
        BluetoothServiceHealth::Reconnecting { .. } => return "reconnecting".into(),
        BluetoothServiceHealth::Degraded { message } => return message.clone(),
        BluetoothServiceHealth::Ready => {}
    }

    if let Some(action) = active_action_text(state.active_action.as_ref()) {
        return action.into();
    }

    let status = &state.snapshot.status;
    if !status.powered {
        "off".into()
    } else if status.discovering {
        "discovering".into()
    } else if status.connected_count > 0 {
        "connected".into()
    } else {
        "ready".into()
    }
}

pub fn prompt_activity_text(prompt: &BluetoothPrompt, snapshot: &BluetoothSnapshot) -> String {
    let label = prompt_device_label(prompt, snapshot);
    match &prompt.kind {
        BluetoothPromptKind::Confirm { .. } => format!("Confirm pairing with {label}"),
        BluetoothPromptKind::AuthorizePairing => format!("Authorize pairing with {label}"),
        BluetoothPromptKind::AuthorizeService { .. } => format!("Authorize {label}"),
        BluetoothPromptKind::RequestPin => format!("Enter PIN for {label}"),
        BluetoothPromptKind::RequestPasskey => format!("Enter passkey for {label}"),
        BluetoothPromptKind::DisplayPin { .. } => format!("Type PIN on {label}"),
        BluetoothPromptKind::DisplayPasskey { .. } => format!("Type passkey on {label}"),
    }
}

pub fn prompt_device_label(prompt: &BluetoothPrompt, snapshot: &BluetoothSnapshot) -> String {
    if !prompt.device_label.is_empty() && prompt.device_label != prompt.device_path {
        return prompt.device_label.clone();
    }

    if let Some(address) = prompt_address(&prompt.device_path) {
        if let Some(device) = snapshot
            .devices
            .iter()
            .find(|device| device.address == address)
        {
            return device.name.clone();
        }
    }

    prompt.device_path.clone()
}

fn prompt_address(path: &str) -> Option<String> {
    let tail = path.rsplit('/').next()?;
    let suffix = tail.strip_prefix("dev_")?;
    Some(suffix.replace('_', ":"))
}

fn active_action_text(action: Option<&BluetoothActiveAction>) -> Option<&'static str> {
    match action? {
        BluetoothActiveAction::SetPowered(true) => Some("turning on"),
        BluetoothActiveAction::SetPowered(false) => Some("turning off"),
        BluetoothActiveAction::SetAdapterPowered { powered: true, .. } => Some("turning on"),
        BluetoothActiveAction::SetAdapterPowered { powered: false, .. } => Some("turning off"),
        BluetoothActiveAction::SetAdapterDiscoverable {
            discoverable: true, ..
        } => Some("discoverable"),
        BluetoothActiveAction::SetAdapterDiscoverable {
            discoverable: false,
            ..
        } => Some("hidden"),
        BluetoothActiveAction::Connect { .. } => Some("connecting"),
        BluetoothActiveAction::Disconnect { .. } => Some("disconnecting"),
        BluetoothActiveAction::Pair { .. } => Some("pairing"),
        BluetoothActiveAction::Trust { trusted: true, .. } => Some("trusting"),
        BluetoothActiveAction::Trust { trusted: false, .. } => Some("untrusting"),
        BluetoothActiveAction::Forget { .. } => Some("forgetting"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::bluetooth::{BluetoothSnapshot, BluetoothStatus};

    #[test]
    fn formats_connected_device_placeholder() {
        let state = State {
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus {
                    powered: true,
                    discovering: false,
                    connected_count: 2,
                },
                ..BluetoothSnapshot::default()
            },
            ..State::default()
        };

        assert_eq!(label("{devices}", &state), "2");
        assert_eq!(
            tooltip("{devices} connected devices", &state),
            "2 connected devices"
        );
    }

    #[test]
    fn formats_state_placeholder() {
        let mut state = State {
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus {
                    powered: true,
                    discovering: true,
                    connected_count: 0,
                },
                ..BluetoothSnapshot::default()
            },
            ..State::default()
        };

        state.health = BluetoothServiceHealth::Ready;
        assert_eq!(label("{state}", &state), "discovering");

        state.snapshot.status.discovering = false;
        assert_eq!(label("{state}", &state), "ready");

        state.snapshot.status.connected_count = 1;
        assert_eq!(label("{state}", &state), "connected");

        state.active_action = Some(BluetoothActiveAction::Connect {
            address: "AA:BB".into(),
        });
        assert_eq!(label("{state}", &state), "connecting");
    }

    #[test]
    fn empty_templates_render_empty_text() {
        let state = State::default();

        assert_eq!(label("", &state), "");
        assert_eq!(tooltip("", &state), "");
    }
}
