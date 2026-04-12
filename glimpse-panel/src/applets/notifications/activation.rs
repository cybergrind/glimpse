use relm4::gtk::{gdk, gio, prelude::*};
use glimpse::compositor::focus_notification_target;

use super::NotificationActionCommand;

pub fn startup_notify_token(desktop_entry: Option<&str>, timestamp: u32) -> Option<String> {
    let display = gdk::Display::default()?;
    let desktop_entry = desktop_entry?;
    let app_info = gio::DesktopAppInfo::new(desktop_entry)
        .or_else(|| gio::DesktopAppInfo::new(&format!("{desktop_entry}.desktop")))?;
    let context = display.app_launch_context();
    context.set_timestamp(timestamp);
    context
        .startup_notify_id(Some(&app_info), &[])
        .map(|token| token.to_string())
}

pub fn invoke_action_command(
    id: u32,
    action_key: &str,
    activation_token: Option<String>,
) -> NotificationActionCommand {
    NotificationActionCommand::InvokeAction {
        id,
        action_key: action_key.to_string(),
        activation_token,
    }
}

pub async fn default_action_command(
    id: u32,
    desktop_entry: Option<String>,
    app_name: String,
    timestamp: u32,
) -> NotificationActionCommand {
    let activation_token = startup_notify_token(desktop_entry.as_deref(), timestamp);
    if activation_token.is_none() {
        let _ = focus_notification_target(desktop_entry.as_deref(), &app_name).await;
    }
    invoke_action_command(id, "default", activation_token)
}

#[cfg(test)]
mod tests {
    use super::invoke_action_command;
    use crate::applets::notifications::NotificationActionCommand;

    #[test]
    fn invoke_action_command_omits_activation_token_when_missing() {
        assert_eq!(
            invoke_action_command(7, "default", None),
            NotificationActionCommand::InvokeAction {
                id: 7,
                action_key: "default".into(),
                activation_token: None,
            }
        );
    }

    #[test]
    fn invoke_action_command_includes_activation_token_when_present() {
        assert_eq!(
            invoke_action_command(7, "default", Some("token-123".into())),
            NotificationActionCommand::InvokeAction {
                id: 7,
                action_key: "default".into(),
                activation_token: Some("token-123".into()),
            }
        );
    }
}
