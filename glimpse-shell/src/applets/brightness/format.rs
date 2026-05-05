use glimpse_core::services::brightness::{BrightnessSource, State};

pub const DEFAULT_LABEL_FORMAT: &str = "";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{source}: {percent}%";
pub const ICON_NAME: &str = "display-brightness-symbolic";

pub fn label(format: &str, state: &State) -> String {
    render(format, primary_source(state))
}

pub fn tooltip(format: &str, state: &State) -> String {
    render(format, primary_source(state))
}

pub fn hero_subtitle(state: &State) -> String {
    primary_source(state)
        .map(|source| source.name.clone())
        .unwrap_or_else(|| "No brightness controls".into())
}

pub fn icon_name(_state: &State) -> &str {
    ICON_NAME
}

fn render(format: &str, source: Option<&BrightnessSource>) -> String {
    if format.is_empty() {
        return String::new();
    }

    let source_name = source
        .map(|source| source.name.as_str())
        .unwrap_or("Brightness");
    let percent = source
        .map(|source| source.percent.to_string())
        .unwrap_or_else(|| "0".into());

    format
        .replace("{source}", source_name)
        .replace("{percent}", &percent)
}

pub fn primary_source(state: &State) -> Option<&BrightnessSource> {
    state
        .sources
        .iter()
        .find(|source| source.primary && source.available && source.writable)
        .or_else(|| {
            state
                .sources
                .iter()
                .find(|source| source.available && source.writable)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::brightness::BrightnessSourceKind;

    #[test]
    fn default_label_is_empty() {
        assert_eq!(DEFAULT_LABEL_FORMAT, "");
    }

    #[test]
    fn tooltip_uses_primary_source() {
        let state = State {
            available: true,
            sources: vec![BrightnessSource {
                id: "backlight:intel_backlight".into(),
                name: "Intel backlight".into(),
                kind: BrightnessSourceKind::BuiltInDisplay,
                icon: "display-brightness-symbolic".into(),
                current: 50,
                max: 100,
                percent: 50,
                writable: true,
                primary: true,
                available: true,
            }],
            active: None,
        };

        assert_eq!(
            tooltip(DEFAULT_TOOLTIP_FORMAT, &state),
            "Intel backlight: 50%"
        );
    }

    #[test]
    fn hero_subtitle_never_includes_percent() {
        let state = State {
            available: true,
            sources: vec![BrightnessSource {
                id: "backlight:intel_backlight".into(),
                name: "Built-in display".into(),
                kind: BrightnessSourceKind::BuiltInDisplay,
                icon: "input-keyboard-symbolic".into(),
                current: 50,
                max: 100,
                percent: 50,
                writable: true,
                primary: true,
                available: true,
            }],
            active: None,
        };

        assert_eq!(hero_subtitle(&state), "Built-in display");
        assert!(!hero_subtitle(&state).contains('%'));
    }

    #[test]
    fn icon_name_is_always_brightness_icon() {
        let state = State {
            available: true,
            sources: vec![BrightnessSource {
                id: "keyboard:upower".into(),
                name: "Keyboard backlight".into(),
                kind: BrightnessSourceKind::Keyboard,
                icon: "input-keyboard-symbolic".into(),
                current: 1,
                max: 3,
                percent: 33,
                writable: true,
                primary: true,
                available: true,
            }],
            active: None,
        };

        assert_eq!(icon_name(&state), ICON_NAME);
    }
}
