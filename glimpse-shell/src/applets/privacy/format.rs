use crate::{
    compositors::{ScreencastSession, ScreencastTarget},
    services::{microphone::MicrophoneUsage, webcam::WebcamUsage},
};

pub fn tooltip(
    microphones: &[MicrophoneUsage],
    webcams: &[WebcamUsage],
    screencasts: &[ScreencastSession],
    location_in_use: bool,
) -> String {
    let mut lines = Vec::new();

    if !microphones.is_empty() {
        lines.push(format!(
            "Microphone: {}",
            app_list(microphones, |usage| { usage.app_name.as_str() })
        ));
    }

    if !webcams.is_empty() {
        lines.push(format!(
            "Camera: {}",
            app_list(webcams, |usage| { usage.app_name.as_str() })
        ));
    }

    if !screencasts.is_empty() {
        lines.push(format!(
            "Screen sharing: {}",
            screencasts
                .iter()
                .map(screencast_label)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if location_in_use {
        lines.push("Location in use".into());
    }

    lines.join("\n")
}

pub fn elapsed(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn app_list<T>(items: &[T], app_name: impl Fn(&T) -> &str) -> String {
    let mut names = Vec::new();

    for item in items {
        let name = app_name(item);
        if !name.is_empty() && !names.iter().any(|existing: &&str| *existing == name) {
            names.push(name);
        }
    }

    if names.is_empty() {
        "active".into()
    } else {
        names.join(", ")
    }
}

fn screencast_label(session: &ScreencastSession) -> &'static str {
    match session.target {
        ScreencastTarget::Monitor => "monitor",
        ScreencastTarget::Window => "window",
        ScreencastTarget::Unknown => "active",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::compositors::{ScreencastKind, ScreencastTarget};

    #[test]
    fn tooltip_lists_active_privacy_sources() {
        let tooltip = tooltip(
            &[
                microphone("Telegram"),
                microphone("Telegram"),
                microphone("Firefox"),
            ],
            &[webcam("Firefox")],
            &[screencast(ScreencastTarget::Monitor)],
            true,
        );

        assert_eq!(
            tooltip,
            "Microphone: Telegram, Firefox\nCamera: Firefox\nScreen sharing: monitor\nLocation in use"
        );
    }

    #[test]
    fn tooltip_is_empty_without_active_sources() {
        assert_eq!(tooltip(&[], &[], &[], false), "");
    }

    #[test]
    fn elapsed_uses_clock_style_minutes_and_seconds() {
        assert_eq!(elapsed(0), "0:00");
        assert_eq!(elapsed(9), "0:09");
        assert_eq!(elapsed(65), "1:05");
    }

    fn microphone(app_name: &str) -> MicrophoneUsage {
        MicrophoneUsage {
            index: 1,
            app_name: app_name.into(),
            app_icon: String::new(),
        }
    }

    fn webcam(app_name: &str) -> WebcamUsage {
        WebcamUsage {
            id: "camera".into(),
            app_name: app_name.into(),
            app_icon: String::new(),
            camera_name: "Camera".into(),
            pipewire_node: None,
        }
    }

    fn screencast(target: ScreencastTarget) -> ScreencastSession {
        ScreencastSession {
            id: "screen".into(),
            session_id: None,
            kind: ScreencastKind::Unknown,
            target,
            active: true,
            pipewire_node: None,
            client_pid: None,
            stoppable: false,
        }
    }
}
