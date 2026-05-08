use gio_unix::DesktopAppInfo;
use relm4::gtk::{gdk, prelude::*};

pub fn startup_notify_token(desktop_entry: Option<&str>, timestamp: u32) -> Option<String> {
    let display = gdk::Display::default()?;
    let app_info = desktop_entry.and_then(desktop_app_info);
    let context = display.app_launch_context();
    context.set_timestamp(timestamp);
    context
        .startup_notify_id(app_info.as_ref(), &[])
        .map(|token| token.to_string())
}

fn desktop_app_info(desktop_entry: &str) -> Option<DesktopAppInfo> {
    desktop_entry_candidates(desktop_entry)
        .into_iter()
        .find_map(|desktop_id| DesktopAppInfo::new(&desktop_id))
}

fn desktop_entry_candidates(desktop_entry: &str) -> Vec<String> {
    let desktop_entry = desktop_entry.trim();
    if desktop_entry.is_empty() {
        return Vec::new();
    }

    let mut candidates = vec![desktop_entry.to_string()];
    if !desktop_entry.ends_with(".desktop") {
        candidates.push(format!("{desktop_entry}.desktop"));
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_entry_candidates_adds_desktop_suffix_when_missing() {
        assert_eq!(
            desktop_entry_candidates("org.example.App"),
            ["org.example.App", "org.example.App.desktop"]
        );
    }

    #[test]
    fn desktop_entry_candidates_keeps_existing_desktop_suffix_single() {
        assert_eq!(
            desktop_entry_candidates("org.example.App.desktop"),
            ["org.example.App.desktop"]
        );
    }

    #[test]
    fn desktop_entry_candidates_ignores_empty_values() {
        assert!(desktop_entry_candidates("  ").is_empty());
    }
}
