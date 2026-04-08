use relm4::gtk::{gdk, gio, prelude::*};

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

pub fn invoke_action_params(
    id: u32,
    action_key: &str,
    activation_token: Option<String>,
) -> serde_json::Value {
    let mut params = serde_json::json!({
        "id": id,
        "action_key": action_key,
    });
    if let Some(token) = activation_token {
        params["activation_token"] = serde_json::Value::String(token);
    }
    params
}

#[cfg(test)]
mod tests {
    use super::invoke_action_params;

    #[test]
    fn invoke_action_params_omits_activation_token_when_missing() {
        assert_eq!(
            invoke_action_params(7, "default", None),
            serde_json::json!({
                "id": 7,
                "action_key": "default",
            })
        );
    }

    #[test]
    fn invoke_action_params_includes_activation_token_when_present() {
        assert_eq!(
            invoke_action_params(7, "default", Some("token-123".into())),
            serde_json::json!({
                "id": 7,
                "action_key": "default",
                "activation_token": "token-123",
            })
        );
    }
}
