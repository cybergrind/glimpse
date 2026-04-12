#![allow(unused_assignments)]

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use glimpse::privacy::{
    PrivacyServiceHandle,
    protocol::{PrivacyServiceCommand, PrivacyServiceState},
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk,
};

use super::components::indicators::{
    PrivacyIndicators, PrivacyIndicatorsInput, PrivacyIndicatorsOutput, PrivacyIndicatorsState,
};
use super::config::PrivacyConfig;

pub struct Privacy {
    service: PrivacyServiceHandle,
    visible: bool,
    mic_active: bool,
    camera_active: bool,
    screen_capture_active: bool,
    screen_capture_started_at: Option<u64>,
    recording_label: String,
    indicators: Controller<PrivacyIndicators>,
}

pub struct PrivacyInit {
    pub config: PrivacyConfig,
    pub service: PrivacyServiceHandle,
}

#[derive(Debug, Clone)]
pub enum PrivacyMsg {
    ServiceState(PrivacyServiceState),
    Tick,
    StopScreenCapture,
    IndicatorsOutput(PrivacyIndicatorsOutput),
    Unavailable,
}

#[derive(Debug, Clone)]
pub enum PrivacyCommandOutput {
    ServiceState(PrivacyServiceState),
    Tick,
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Privacy {
    type Init = PrivacyInit;
    type Input = PrivacyMsg;
    type Output = ();
    type CommandOutput = PrivacyCommandOutput;

    view! {
        gtk::Box {
            #[local_ref]
            indicators_widget -> gtk::Box {},
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let _ = init.config;
        let indicators = PrivacyIndicators::builder()
            .launch(())
            .forward(sender.input_sender(), PrivacyMsg::IndicatorsOutput);
        let indicators_widget = indicators.widget().clone();

        let model = Privacy {
            service: init.service.clone(),
            visible: false,
            mic_active: false,
            camera_active: false,
            screen_capture_active: false,
            screen_capture_started_at: None,
            recording_label: String::new(),
            indicators,
        };

        let service = init.service;
        sender.command(move |out, shutdown| {
            out.send(PrivacyCommandOutput::Tick).ok();
            shutdown
                .register(async move {
                    tracing::info!("privacy applet: subscribing to privacy service");
                    let mut state_rx = service.subscribe();
                    let _ = out.send(PrivacyCommandOutput::ServiceState(state_rx.borrow().clone()));

                    loop {
                        tokio::select! {
                            changed = state_rx.changed() => {
                                if changed.is_err() {
                                    break;
                                }
                                let _ = out.send(PrivacyCommandOutput::ServiceState(state_rx.borrow().clone()));
                            }
                            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                                if out.send(PrivacyCommandOutput::Tick).is_err() {
                                    break;
                                }
                            }
                        }
                    }

                    let _ = out.send(PrivacyCommandOutput::Unavailable);
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            PrivacyCommandOutput::ServiceState(state) => {
                self.update(PrivacyMsg::ServiceState(state), sender, root);
            }
            PrivacyCommandOutput::Tick => self.update(PrivacyMsg::Tick, sender, root),
            PrivacyCommandOutput::Unavailable => self.update(PrivacyMsg::Unavailable, sender, root),
        }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            PrivacyMsg::ServiceState(state) => {
                self.mic_active = state.snapshot.mic_active;
                self.camera_active = state.snapshot.camera_active;
                self.screen_capture_active = state.snapshot.screen_capture_active;
                self.screen_capture_started_at = state.snapshot.oldest_screen_capture_started_at;
                self.recording_label =
                    format_elapsed_label(self.screen_capture_started_at, now_unix_secs());
                self.visible = should_show(
                    self.mic_active,
                    self.camera_active,
                    self.screen_capture_active,
                );
                self.sync_indicators();
            }
            PrivacyMsg::Tick => {
                if self.screen_capture_active {
                    self.recording_label =
                        format_elapsed_label(self.screen_capture_started_at, now_unix_secs());
                    self.sync_indicators();
                }
            }
            PrivacyMsg::StopScreenCapture => {
                if should_stop_screen_capture_on_click(self.screen_capture_active, 1) {
                    self.send_command(sender, PrivacyServiceCommand::StopAllScreenCapture);
                }
            }
            PrivacyMsg::IndicatorsOutput(PrivacyIndicatorsOutput::StopScreenCaptureRequested) => {
                sender.input(PrivacyMsg::StopScreenCapture);
            }
            PrivacyMsg::Unavailable => {
                tracing::warn!("privacy applet: privacy service unavailable");
                self.visible = false;
                self.sync_indicators();
            }
        }
    }
}

impl Privacy {
    fn sync_indicators(&self) {
        self.indicators.emit(PrivacyIndicatorsInput::Update(PrivacyIndicatorsState {
            visible: self.visible,
            mic_active: self.mic_active,
            camera_active: self.camera_active,
            screen_capture_active: self.screen_capture_active,
            recording_label: self.recording_label.clone(),
        }));
    }

    fn send_command(&self, sender: ComponentSender<Self>, command: PrivacyServiceCommand) {
        let service = self.service.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    if let Err(error) = service.send(command).await {
                        tracing::warn!(%error, "privacy applet: failed to send privacy service command");
                    }
                })
                .drop_on_shutdown()
        });
    }
}

fn should_show(mic_active: bool, camera_active: bool, screen_capture_active: bool) -> bool {
    mic_active || camera_active || screen_capture_active
}

fn should_stop_screen_capture_on_click(screen_capture_active: bool, button: u32) -> bool {
    screen_capture_active && button == 1
}

fn format_elapsed_label(started_at: Option<u64>, now_secs: u64) -> String {
    let elapsed = started_at
        .map(|started_at| now_secs.saturating_sub(started_at))
        .unwrap_or(0);
    let minutes = elapsed / 60;
    let seconds = elapsed % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{format_elapsed_label, should_show, should_stop_screen_capture_on_click};

    #[test]
    fn elapsed_label_uses_mm_ss() {
        assert_eq!(format_elapsed_label(Some(100), 105), "00:05");
        assert_eq!(format_elapsed_label(Some(100), 165), "01:05");
    }

    #[test]
    fn elapsed_label_defaults_to_zero_without_start_time() {
        assert_eq!(format_elapsed_label(None, 500), "00:00");
    }

    #[test]
    fn applet_is_hidden_when_nothing_is_active() {
        assert!(!should_show(false, false, false));
        assert!(should_show(true, false, false));
        assert!(should_show(false, true, false));
        assert!(should_show(false, false, true));
    }

    #[test]
    fn stop_sharing_click_only_uses_primary_button_while_recording() {
        assert!(should_stop_screen_capture_on_click(true, 1));
        assert!(!should_stop_screen_capture_on_click(true, 3));
        assert!(!should_stop_screen_capture_on_click(false, 1));
    }
}
