use glimpse_core::services::session::{SessionAction, SessionServiceHealth, State};

pub const DEFAULT_LABEL_FORMAT: &str = "{user}";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{user} on {host}";

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
        SessionServiceHealth::Degraded { message } => return message.clone(),
        SessionServiceHealth::Ready => {}
    }

    if let Some(action) = state.active_action {
        return action_text(action).into();
    }

    "ready".into()
}

fn render(template: &str, state: &State) -> String {
    if template.is_empty() {
        return String::new();
    }

    let snapshot = &state.snapshot;
    template
        .replace("{user}", &snapshot.user_name)
        .replace("{host}", &snapshot.host_name)
        .replace("{uptime}", &snapshot.uptime)
        .replace("{state}", &state_text(state))
        .trim()
        .to_owned()
}

fn action_text(action: SessionAction) -> &'static str {
    match action {
        SessionAction::Lock => "locking",
        SessionAction::Logout => "logging out",
        SessionAction::Suspend => "suspending",
        SessionAction::Hibernate => "hibernating",
        SessionAction::Reboot => "restarting",
        SessionAction::PowerOff => "shutting down",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::session::{
        SessionAction, SessionServiceHealth, SessionSnapshot, State,
    };

    #[test]
    fn formats_session_placeholders() {
        let state = State {
            snapshot: SessionSnapshot {
                user_name: "alex".into(),
                host_name: "workstation".into(),
                uptime: "2h 5m".into(),
                ..SessionSnapshot::default()
            },
            health: SessionServiceHealth::Ready,
            ..State::default()
        };

        assert_eq!(
            label("{user}@{host} {uptime} {state}", &state),
            "alex@workstation 2h 5m ready"
        );
        assert_eq!(
            tooltip(DEFAULT_TOOLTIP_FORMAT, &state),
            "alex on workstation"
        );
    }

    #[test]
    fn state_placeholder_prefers_health_and_active_action() {
        let degraded = State {
            health: SessionServiceHealth::Degraded {
                message: "Session actions unavailable".into(),
            },
            ..State::default()
        };
        assert_eq!(label("{state}", &degraded), "Session actions unavailable");

        let active = State {
            health: SessionServiceHealth::Ready,
            active_action: Some(SessionAction::PowerOff),
            ..State::default()
        };
        assert_eq!(label("{state}", &active), "shutting down");
    }
}
