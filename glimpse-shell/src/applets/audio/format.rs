use glimpse_core::services::audio::State;

pub const DEFAULT_LABEL_FORMAT: &str = "";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{device} - {volume}%";

pub fn label(template: &str, state: &State) -> String {
    render(template, state)
}

pub fn tooltip(template: &str, state: &State) -> String {
    render(template, state)
}

fn render(template: &str, state: &State) -> String {
    if template.is_empty() {
        return String::new();
    }

    let output = state.default_output();
    let input = state.default_input();
    template
        .replace("{state}", state_label(state))
        .replace(
            "{volume}",
            &output
                .map(|device| device.volume.to_string())
                .unwrap_or_default(),
        )
        .replace(
            "{device}",
            output
                .map(|device| device.description.as_str())
                .unwrap_or(""),
        )
        .replace(
            "{input_volume}",
            &input
                .map(|device| device.volume.to_string())
                .unwrap_or_default(),
        )
        .replace(
            "{input_device}",
            input
                .map(|device| device.description.as_str())
                .unwrap_or(""),
        )
        .trim_end_matches([' ', ',', '-'])
        .to_owned()
}

fn state_label(state: &State) -> &'static str {
    if !state.available {
        "unavailable"
    } else if state.default_output().is_some() {
        "ready"
    } else {
        "no-output"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::audio::AudioDevice;

    #[test]
    fn default_label_is_empty() {
        assert_eq!(label(DEFAULT_LABEL_FORMAT, &State::default()), "");
    }

    #[test]
    fn tooltip_uses_default_output_placeholders() {
        let mut state = State {
            available: true,
            ..State::default()
        };
        state.outputs.push(AudioDevice {
            index: 1,
            name: "sink".into(),
            description: "Speakers".into(),
            volume: 64,
            muted: false,
            is_default: true,
            icon_name: "audio-speakers-symbolic".into(),
        });

        assert_eq!(tooltip(DEFAULT_TOOLTIP_FORMAT, &state), "Speakers - 64%");
        assert_eq!(label("{state}", &state), "ready");
    }
}
