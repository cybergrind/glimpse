use glimpse::providers::audio::{AudioDevice, AudioEvent, AudioStream};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SoundState {
    outputs: Vec<AudioDevice>,
    inputs: Vec<AudioDevice>,
    streams: Vec<AudioStream>,
}

impl SoundState {
    pub fn new(outputs: Vec<AudioDevice>, inputs: Vec<AudioDevice>, streams: Vec<AudioStream>) -> Self {
        Self {
            outputs,
            inputs,
            streams,
        }
    }

    pub fn replace(&mut self, outputs: Vec<AudioDevice>, inputs: Vec<AudioDevice>) {
        self.outputs = outputs;
        self.inputs = inputs;
    }

    pub fn outputs(&self) -> &[AudioDevice] {
        &self.outputs
    }

    pub fn inputs(&self) -> &[AudioDevice] {
        &self.inputs
    }

    pub fn streams(&self) -> &[AudioStream] {
        &self.streams
    }

    pub fn default_output(&self) -> Option<&AudioDevice> {
        default_device(&self.outputs)
    }

    pub fn default_input(&self) -> Option<&AudioDevice> {
        default_device(&self.inputs)
    }

    pub fn select_output(&mut self, name: &str) -> bool {
        mark_default(&mut self.outputs, name)
    }

    pub fn select_input(&mut self, name: &str) -> bool {
        mark_default(&mut self.inputs, name)
    }

    pub fn set_output_volume(&mut self, value: u32) {
        if let Some(device) = default_device_mut(&mut self.outputs) {
            device.volume = value.min(100);
        }
    }

    pub fn set_input_volume(&mut self, value: u32) {
        if let Some(device) = default_device_mut(&mut self.inputs) {
            device.volume = value.min(100);
        }
    }

    pub fn set_output_muted(&mut self, muted: bool) {
        if let Some(device) = default_device_mut(&mut self.outputs) {
            device.muted = muted;
        }
    }

    pub fn set_input_muted(&mut self, muted: bool) {
        if let Some(device) = default_device_mut(&mut self.inputs) {
            device.muted = muted;
        }
    }

    pub fn update_outputs(&mut self, outputs: Vec<AudioDevice>) {
        self.outputs = outputs;
    }

    pub fn update_inputs(&mut self, inputs: Vec<AudioDevice>) {
        self.inputs = inputs;
    }

    pub fn update_streams(&mut self, streams: Vec<AudioStream>) {
        self.streams = streams;
    }

    pub fn set_stream_volume(&mut self, index: u64, value: u32) {
        if let Some(stream) = self.streams.iter_mut().find(|stream| stream.index == index) {
            stream.volume = value.min(100);
        }
    }

    pub fn set_stream_muted(&mut self, index: u64, muted: bool) {
        if let Some(stream) = self.streams.iter_mut().find(|stream| stream.index == index) {
            stream.muted = muted;
        }
    }

    pub fn stream(&self, index: u64) -> Option<&AudioStream> {
        self.streams.iter().find(|stream| stream.index == index)
    }

    pub fn apply_event(&mut self, event: &AudioEvent) -> bool {
        match event {
            AudioEvent::OutputsChanged(outputs) => {
                self.update_outputs(outputs.iter().cloned().collect());
                true
            }
            AudioEvent::InputsChanged(inputs) => {
                self.update_inputs(inputs.iter().cloned().collect());
                true
            }
            AudioEvent::StreamsChanged(streams) => {
                self.update_streams(streams.clone());
                true
            }
            AudioEvent::Unavailable => false,
        }
    }
}

fn default_device(devices: &[AudioDevice]) -> Option<&AudioDevice> {
    devices.iter().find(|device| device.is_default).or_else(|| devices.first())
}

fn default_device_mut(devices: &mut [AudioDevice]) -> Option<&mut AudioDevice> {
    if let Some(index) = devices.iter().position(|device| device.is_default) {
        return devices.get_mut(index);
    }
    devices.first_mut()
}

fn mark_default(devices: &mut [AudioDevice], name: &str) -> bool {
    let Some(index) = devices.iter().position(|device| device.name == name) else {
        return false;
    };

    for device in devices.iter_mut() {
        device.is_default = false;
    }
    devices[index].is_default = true;
    true
}

#[cfg(test)]
mod tests {
    use super::SoundState;
    use glimpse::providers::audio::{AudioDevice, AudioEvent, AudioStream};

    #[test]
    fn prefers_explicit_default_devices() {
        let state = SoundState::new(
            vec![
                device("speakers", "Built-in Speakers", 28, false, false),
                device("headset", "USB Headset", 72, true, true),
            ],
            vec![
                device("internal-mic", "Internal Microphone", 48, false, false),
                device("usb-mic", "USB Microphone", 64, false, true),
            ],
            vec![],
        );

        assert_eq!(
            state.default_output().expect("default output").description,
            "USB Headset"
        );
        assert_eq!(
            state.default_input().expect("default input").description,
            "USB Microphone"
        );
    }

    #[test]
    fn falls_back_to_first_device_when_no_default_is_flagged() {
        let state = SoundState::new(
            vec![device("speakers", "Built-in Speakers", 55, false, false)],
            vec![device("internal-mic", "Internal Microphone", 61, false, false)],
            vec![],
        );

        assert_eq!(
            state.default_output().expect("fallback output").description,
            "Built-in Speakers"
        );
        assert_eq!(
            state.default_input().expect("fallback input").description,
            "Internal Microphone"
        );
    }

    #[test]
    fn selecting_a_new_default_clears_the_old_default() {
        let mut state = SoundState::new(
            vec![
                device("speakers", "Built-in Speakers", 28, false, true),
                device("headset", "USB Headset", 72, false, false),
            ],
            vec![],
            vec![],
        );

        assert!(state.select_output("headset"));

        assert_eq!(
            state.default_output().expect("new default").description,
            "USB Headset"
        );
        assert!(state.outputs()[1].is_default);
        assert!(!state.outputs()[0].is_default);
    }

    #[test]
    fn volume_and_mute_updates_apply_to_the_current_default_devices() {
        let mut state = SoundState::new(
            vec![
                device("speakers", "Built-in Speakers", 28, false, false),
                device("headset", "USB Headset", 72, false, true),
            ],
            vec![device("usb-mic", "USB Microphone", 64, false, true)],
            vec![],
        );

        state.set_output_volume(140);
        state.set_output_muted(true);
        state.set_input_volume(12);
        state.set_input_muted(true);

        assert_eq!(state.default_output().expect("output").volume, 100);
        assert!(state.default_output().expect("output").muted);
        assert_eq!(state.default_input().expect("input").volume, 12);
        assert!(state.default_input().expect("input").muted);
    }

    #[test]
    fn replaces_outputs_when_provider_reports_a_new_device_list() {
        let mut state = SoundState::new(
            vec![device("speakers", "Built-in Speakers", 55, false, true)],
            vec![],
            vec![],
        );

        state.update_outputs(vec![
            device("speakers", "Built-in Speakers", 55, false, false),
            device("headset", "USB Headset", 72, false, true),
        ]);

        assert_eq!(state.outputs().len(), 2);
        assert_eq!(
            state.default_output().expect("default output").description,
            "USB Headset"
        );
    }

    #[test]
    fn unavailable_event_does_not_mutate_page_state() {
        let mut state = SoundState::new(
            vec![device("speakers", "Built-in Speakers", 55, false, true)],
            vec![device("mic", "Internal Microphone", 61, false, true)],
            vec![],
        );

        assert!(!state.apply_event(&AudioEvent::Unavailable));
        assert_eq!(state.outputs().len(), 1);
        assert_eq!(state.inputs().len(), 1);
        assert!(state.streams().is_empty());
    }

    #[test]
    fn applies_stream_events_and_updates_stream_controls() {
        let mut state = SoundState::new(vec![], vec![], vec![]);

        assert!(state.apply_event(&AudioEvent::StreamsChanged(vec![
            stream(7, "Firefox", 42, false),
            stream(9, "Spotify", 63, true),
        ])));
        assert_eq!(state.streams().len(), 2);

        state.set_stream_volume(7, 150);
        state.set_stream_muted(9, false);

        assert_eq!(state.streams()[0].volume, 100);
        assert!(!state.streams()[1].muted);
    }

    fn device(
        name: &str,
        description: &str,
        volume: u32,
        muted: bool,
        is_default: bool,
    ) -> AudioDevice {
        AudioDevice {
            index: 1,
            name: name.into(),
            description: description.into(),
            volume,
            muted,
            is_default,
            icon_name: "audio-speakers-symbolic".into(),
        }
    }

    fn stream(index: u64, app_name: &str, volume: u32, muted: bool) -> AudioStream {
        AudioStream {
            index,
            app_name: app_name.into(),
            app_icon: "audio-x-generic-symbolic".into(),
            volume,
            muted,
        }
    }
}
