#![allow(unused_assignments)]

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        compositor::{Command as CompositorCommand, CompositorHandle, State as CompositorState},
        framework::ServiceCommand,
        geoclue::{GeoClueHandle, State as GeoClueState},
        microphone::{MicrophoneHandle, State as MicrophoneState},
        webcam::{State as WebcamState, WebcamHandle},
    },
};

use super::{components::indicators::PrivacyIndicators, format};

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid privacy applet config, using defaults");
                Self::default()
            }
        }
    }
}

pub struct Applet {
    config: Config,
    state: PrivacyState,
    view: View,
    microphone: MicrophoneHandle,
    webcam: WebcamHandle,
    compositor: CompositorHandle,
    geoclue: GeoClueHandle,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub microphone: MicrophoneHandle,
    pub webcam: WebcamHandle,
    pub compositor: CompositorHandle,
    pub geoclue: GeoClueHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    MicrophoneStateChanged(MicrophoneState),
    WebcamStateChanged(WebcamState),
    CompositorStateChanged(CompositorState),
    GeoClueStateChanged(GeoClueState),
    Reconfigure(Config),
    Activate,
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            set_orientation: gtk::Orientation::Horizontal,
            set_valign: gtk::Align::Center,
            #[watch]
            set_visible: model.view.visible,
            #[watch]
            set_tooltip_text: if model.view.tooltip.is_empty() {
                None
            } else {
                Some(&model.view.tooltip)
            },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::Activate);
                },
            },

            #[template]
            PrivacyIndicators {
                #[template_child]
                microphone {
                    #[watch]
                    set_visible: model.view.microphone_visible,
                },

                #[template_child]
                camera {
                    #[watch]
                    set_visible: model.view.camera_visible,
                },

                #[template_child]
                screen {
                    #[watch]
                    set_visible: model.view.screen_visible,
                },

                #[template_child]
                location {
                    #[watch]
                    set_visible: model.view.location_visible,
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let state = PrivacyState::from_services(
            &init.microphone.snapshot(),
            &init.webcam.snapshot(),
            &init.compositor.snapshot(),
            &init.geoclue.snapshot(),
        );
        let view = view_from_state(&state);
        let model = Applet {
            config: init.config,
            state,
            view,
            microphone: init.microphone,
            webcam: init.webcam,
            compositor: init.compositor,
            geoclue: init.geoclue,
            subscription_cancel: CancellationToken::new(),
        };

        spawn_subscriptions(&model, &sender);

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::MicrophoneStateChanged(state) => {
                if self.state.microphones != state.usages {
                    self.state.microphones = state.usages;
                    self.sync_view();
                }
            }
            Input::WebcamStateChanged(state) => {
                if self.state.webcams != state.usages {
                    self.state.webcams = state.usages;
                    self.sync_view();
                }
            }
            Input::CompositorStateChanged(state) => {
                let screencasts = active_screencasts(&state);
                if self.state.screencasts != screencasts {
                    self.state.screencasts = screencasts;
                    self.sync_view();
                }
            }
            Input::GeoClueStateChanged(state) => {
                if self.state.location_in_use != state.in_use {
                    self.state.location_in_use = state.in_use;
                    self.sync_view();
                }
            }
            Input::Reconfigure(config) => {
                if self.config != config {
                    self.config = config;
                    self.sync_view();
                }
            }
            Input::Activate => {
                self.stop_stoppable_screencasts();
            }
        }
    }
}

impl Applet {
    fn sync_view(&mut self) {
        let view = view_from_state(&self.state);
        if self.view != view {
            self.view = view;
        }
    }

    fn stop_stoppable_screencasts(&self) {
        let commands = self
            .state
            .screencasts
            .iter()
            .filter_map(stoppable_screencast_id)
            .map(CompositorCommand::StopScreencast)
            .collect::<Vec<_>>();

        if commands.is_empty() {
            return;
        }

        let compositor = self.compositor.clone();
        relm4::spawn(async move {
            for command in commands {
                if let Err(error) = compositor.send(ServiceCommand::Command(command)).await {
                    tracing::warn!(
                        %error,
                        "failed to send compositor command from privacy applet"
                    );
                    break;
                }
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PrivacyState {
    microphones: Vec<glimpse_core::services::microphone::MicrophoneUsage>,
    webcams: Vec<glimpse_core::services::webcam::WebcamUsage>,
    screencasts: Vec<glimpse_core::compositors::ScreencastSession>,
    location_in_use: bool,
}

impl PrivacyState {
    fn from_services(
        microphone: &MicrophoneState,
        webcam: &WebcamState,
        compositor: &CompositorState,
        geoclue: &GeoClueState,
    ) -> Self {
        Self {
            microphones: microphone.usages.clone(),
            webcams: webcam.usages.clone(),
            screencasts: active_screencasts(compositor),
            location_in_use: geoclue.in_use,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct View {
    visible: bool,
    microphone_visible: bool,
    camera_visible: bool,
    screen_visible: bool,
    location_visible: bool,
    tooltip: String,
}

fn view_from_state(state: &PrivacyState) -> View {
    let microphone_visible = !state.microphones.is_empty();
    let camera_visible = !state.webcams.is_empty();
    let screen_visible = !state.screencasts.is_empty();
    let location_visible = state.location_in_use;

    View {
        visible: microphone_visible || camera_visible || screen_visible || location_visible,
        microphone_visible,
        camera_visible,
        screen_visible,
        location_visible,
        tooltip: format::tooltip(
            &state.microphones,
            &state.webcams,
            &state.screencasts,
            state.location_in_use,
        ),
    }
}

fn active_screencasts(
    state: &CompositorState,
) -> Vec<glimpse_core::compositors::ScreencastSession> {
    state
        .screencasts
        .iter()
        .filter(|session| session.active)
        .cloned()
        .collect()
}

fn stoppable_screencast_id(
    session: &glimpse_core::compositors::ScreencastSession,
) -> Option<String> {
    if !session.stoppable {
        return None;
    }

    Some(session.session_id.as_ref().unwrap_or(&session.id).clone())
}

fn spawn_subscriptions(model: &Applet, sender: &ComponentSender<Applet>) {
    let mut microphone = model.microphone.subscribe();
    let mut webcam = model.webcam.subscribe();
    let mut compositor = model.compositor.subscribe();
    let mut geoclue = model.geoclue.subscribe();
    let cancel = model.subscription_cancel.clone();
    let sender = sender.clone();

    relm4::spawn(async move {
        sender.input(Input::MicrophoneStateChanged(microphone.borrow().clone()));
        sender.input(Input::WebcamStateChanged(webcam.borrow().clone()));
        sender.input(Input::CompositorStateChanged(compositor.borrow().clone()));
        sender.input(Input::GeoClueStateChanged(geoclue.borrow().clone()));

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = microphone.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    sender.input(Input::MicrophoneStateChanged(microphone.borrow().clone()));
                }
                changed = webcam.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    sender.input(Input::WebcamStateChanged(webcam.borrow().clone()));
                }
                changed = compositor.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    sender.input(Input::CompositorStateChanged(compositor.borrow().clone()));
                }
                changed = geoclue.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    sender.input(Input::GeoClueStateChanged(geoclue.borrow().clone()));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        compositors::{ScreencastKind, ScreencastSession, ScreencastTarget},
        services::{microphone::MicrophoneUsage, webcam::WebcamUsage},
    };

    #[test]
    fn config_accepts_absent_and_empty_settings() {
        assert_eq!(Config::from_raw(&None), Config::default());
        assert_eq!(
            Config::from_raw(&Some(AppletConfig::default())),
            Config::default()
        );
    }

    #[test]
    fn view_hides_without_active_privacy_sources() {
        assert!(!view_from_state(&PrivacyState::default()).visible);
    }

    #[test]
    fn view_shows_active_privacy_sources() {
        let state = PrivacyState {
            microphones: vec![microphone()],
            webcams: vec![webcam()],
            screencasts: vec![screencast(true)],
            location_in_use: true,
        };
        let view = view_from_state(&state);

        assert!(view.visible);
        assert!(view.microphone_visible);
        assert!(view.camera_visible);
        assert!(view.screen_visible);
        assert!(view.location_visible);
        assert!(view.tooltip.contains("Microphone: Telegram"));
    }

    #[test]
    fn active_screencasts_filters_inactive_sessions() {
        let state = CompositorState {
            screencasts: vec![screencast(true), screencast(false)],
            ..CompositorState::default()
        };

        assert_eq!(active_screencasts(&state).len(), 1);
    }

    #[test]
    fn stoppable_screencast_id_prefers_session_id() {
        let mut screencast = screencast(true);
        screencast.stoppable = true;
        screencast.id = "stream".into();
        screencast.session_id = Some("session".into());

        assert_eq!(stoppable_screencast_id(&screencast), Some("session".into()));

        screencast.session_id = None;
        assert_eq!(stoppable_screencast_id(&screencast), Some("stream".into()));

        screencast.stoppable = false;
        assert_eq!(stoppable_screencast_id(&screencast), None);
    }

    fn microphone() -> MicrophoneUsage {
        MicrophoneUsage {
            index: 1,
            app_name: "Telegram".into(),
            app_icon: String::new(),
        }
    }

    fn webcam() -> WebcamUsage {
        WebcamUsage {
            id: "camera".into(),
            app_name: "Firefox".into(),
            app_icon: String::new(),
            camera_name: "Camera".into(),
            pipewire_node: None,
        }
    }

    fn screencast(active: bool) -> ScreencastSession {
        ScreencastSession {
            id: format!("screen-{active}"),
            session_id: None,
            kind: ScreencastKind::Unknown,
            target: ScreencastTarget::Window,
            active,
            pipewire_node: None,
            client_pid: None,
            stoppable: false,
        }
    }
}
