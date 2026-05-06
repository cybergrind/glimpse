use glimpse_core::services::storage::State;

pub fn label(template: &str, state: &State) -> String {
    render(template, state)
}

pub fn tooltip(template: &str, state: &State) -> String {
    render(template, state)
}

fn render(template: &str, state: &State) -> String {
    let count = state.devices.len();
    let mounted = state
        .devices
        .iter()
        .filter(|device| device.mounted_at.is_some())
        .count();

    template
        .replace("{count}", &count.to_string())
        .replace("{mounted}", &mounted.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::storage::StorageDevice;

    #[test]
    fn formats_device_counts() {
        let mut state = State::default();
        state.devices = vec![
            StorageDevice {
                id: "one".into(),
                name: "One".into(),
                mounted_at: Some("/run/media/one".into()),
                ..StorageDevice::default()
            },
            StorageDevice {
                id: "two".into(),
                name: "Two".into(),
                ..StorageDevice::default()
            },
        ];

        assert_eq!(label("{count}", &state), "2");
        assert_eq!(tooltip("{mounted}/{count} mounted", &state), "1/2 mounted");
    }
}
