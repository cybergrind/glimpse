use glimpse_core::services::notifications::model::{NotificationEntry, State};

pub const DEFAULT_LABEL_FORMAT: &str = "";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{count} notifications";

pub fn icon_name(state: &State) -> &'static str {
    if state.dnd {
        "notifications-disabled-symbolic"
    } else {
        "preferences-system-notifications-symbolic"
    }
}

pub fn label(format: &str, state: &State) -> String {
    render(format, state.notifications.len(), state.dnd)
}

pub fn tooltip(format: &str, state: &State) -> String {
    if state.dnd {
        return "Do Not Disturb".into();
    }

    render(format, state.notifications.len(), state.dnd)
}

pub fn count_label(count: usize) -> String {
    match count {
        0 => "No notifications".into(),
        1 => "1 notification".into(),
        count => format!("{count} notifications"),
    }
}

pub fn source_name(notification: &NotificationEntry) -> &str {
    if notification.app_name.is_empty() {
        "Notification"
    } else {
        &notification.app_name
    }
}

pub fn relative_time(now_ms: u64, timestamp_ms: u64) -> String {
    let elapsed = now_ms.saturating_sub(timestamp_ms) / 1000;
    match elapsed {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", elapsed / 60),
        3600..=86399 => format!("{}h", elapsed / 3600),
        _ => format!("{}d", elapsed / 86400),
    }
}

pub fn visible_actions(notification: &NotificationEntry) -> impl Iterator<Item = (&str, &str)> {
    notification
        .actions
        .iter()
        .filter(|action| action.key != "default")
        .map(|action| (action.key.as_str(), action.label.as_str()))
}

fn render(format: &str, count: usize, dnd: bool) -> String {
    format
        .replace("{count}", &count.to_string())
        .replace("{state}", if dnd { "dnd" } else { "enabled" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::notifications::model::State;

    #[test]
    fn renders_count_and_state_placeholders() {
        let state = State {
            dnd: true,
            ..State::default()
        };

        assert_eq!(label("{count}:{state}", &state), "0:dnd");
    }
}
