use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::Config;
use crate::{
    dbus::Dbus,
    services::{
        audio, audio_events, battery, bluetooth, brightness, calendar_events, clipboard, clock,
        compositor, geoclue, keyboard, location, microphone, mpris, network, notifications, power,
        session, tray, weather, webcam,
    },
};

macro_rules! for_each_service_handle {
    ($self:expr, $control:expr, [$($name:ident),* $(,)?]) => {
        $(
            if let Err(err) = $self
                .$name
                .try_send(ServiceCommand::Control($control.clone()))
            {
                tracing::warn!(
                    service = stringify!($name),
                    %err,
                    "failed to broadcast control"
                );
            }
        )*
    };
}

#[derive(Clone)]
pub enum Control {
    Start(Config),
    Reconfigure(Config),
    Shutdown,
}

pub enum ServiceCommand<Command> {
    Command(Command),
    Control(Control),
}

#[derive(Debug, Clone)]
pub struct ServiceHandle<State, Command> {
    state_rx: watch::Receiver<State>,
    command_tx: mpsc::Sender<ServiceCommand<Command>>,
}

#[derive(Debug)]
pub enum ServiceError {
    ChannelClosed,
    ChannelFull,
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChannelClosed => f.write_str("service channel closed"),
            Self::ChannelFull => f.write_str("service channel full"),
        }
    }
}

impl std::error::Error for ServiceError {}

impl<State, Command> ServiceHandle<State, Command> {
    pub fn new(
        state_rx: watch::Receiver<State>,
        command_tx: mpsc::Sender<ServiceCommand<Command>>,
    ) -> Self {
        Self {
            state_rx,
            command_tx,
        }
    }
    pub fn subscribe(&self) -> watch::Receiver<State> {
        self.state_rx.clone()
    }
}

impl<State: Clone, Command: Send> ServiceHandle<State, Command> {
    pub fn snapshot(&self) -> State {
        self.state_rx.borrow().clone()
    }

    pub async fn send(&self, command: ServiceCommand<Command>) -> Result<(), ServiceError> {
        self.command_tx
            .send(command)
            .await
            .map_err(|_| ServiceError::ChannelClosed)
    }

    pub fn try_send(&self, command: ServiceCommand<Command>) -> Result<(), ServiceError> {
        self.command_tx.try_send(command).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => ServiceError::ChannelFull,
            mpsc::error::TrySendError::Closed(_) => ServiceError::ChannelClosed,
        })
    }
}

pub struct RunningService {
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl RunningService {
    pub async fn cancel(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }
}

#[derive(Clone)]
pub struct Services {
    pub audio: ServiceHandle<audio::State, audio::Command>,
    pub audio_events: audio_events::AudioEventsHandle,
    pub clock: clock::ClockHandle,
    pub calendar_events: calendar_events::CalendarEventsHandle,
    pub geoclue: geoclue::GeoClueHandle,
    pub location: ServiceHandle<location::State, location::Command>,
    pub microphone: microphone::MicrophoneHandle,
    pub mpris: mpris::MprisHandle,
    pub battery: ServiceHandle<battery::State, battery::Command>,
    pub brightness: brightness::BrightnessHandle,
    pub clipboard: clipboard::ClipboardHandle,
    pub power: ServiceHandle<power::State, power::Command>,
    pub bluetooth: ServiceHandle<bluetooth::State, bluetooth::Command>,
    pub network: network::NetworkHandle,
    pub notifications: notifications::NotificationsHandle,
    pub session: session::SessionHandle,
    pub compositor: compositor::CompositorHandle,
    pub keyboard: keyboard::KeyboardHandle,
    pub weather: weather::WeatherHandle,
    pub tray: tray::TrayHandle,
    pub webcam: webcam::WebcamHandle,
    pub system_dbus: zbus::Connection,
    pub session_dbus: zbus::Connection,
}

impl Services {
    pub fn broadcast(&self, control: Control) {
        for_each_service_handle!(
            self,
            control,
            [
                audio,
                audio_events,
                clock,
                calendar_events,
                geoclue,
                location,
                microphone,
                mpris,
                battery,
                brightness,
                clipboard,
                power,
                bluetooth,
                network,
                notifications,
                session,
                compositor,
                keyboard,
                weather,
                tray,
                webcam
            ]
        );
    }
}

pub struct ServiceRuntime {
    handles: Services,
    running_services: Vec<RunningService>,
}

impl ServiceRuntime {
    pub fn new(dbus: Dbus) -> Self {
        let session_dbus = dbus.session;
        let system_dbus = dbus.system;

        let (audio_events_service, audio_events) = audio_events::AudioEventsService::new();
        let audio_events_service = spawn_service(|cancel| audio_events_service.run(cancel));

        let (audio_service, audio) = audio::AudioService::new(audio_events.clone());
        let audio_service = spawn_service(|cancel| audio_service.run(cancel));

        let (clock_service, clock) = clock::ClockService::new();
        let clock_service = spawn_service(|cancel| clock_service.run(cancel));

        let (calendar_events_service, calendar_events) =
            calendar_events::CalendarEventsService::new(session_dbus.clone());
        let calendar_events_service = spawn_service(|cancel| calendar_events_service.run(cancel));

        let (geoclue_service, geoclue) = geoclue::GeoClueService::new(system_dbus.clone());
        let geoclue_service = spawn_service(|cancel| geoclue_service.run(cancel));

        let (location_service, location) = location::LocationService::new(geoclue.clone());
        let location_service = spawn_service(|cancel| location_service.run(cancel));

        let (microphone_service, microphone) =
            microphone::MicrophoneService::new(audio_events.clone());
        let microphone_service = spawn_service(|cancel| microphone_service.run(cancel));

        let (mpris_service, mpris) = mpris::MprisService::new(session_dbus.clone());
        let mpris_service = spawn_service(|cancel| mpris_service.run(cancel));

        let (battery_service, battery) = battery::BatteryService::new(system_dbus.clone());
        let battery_service = spawn_service(|cancel| battery_service.run(cancel));

        let (brightness_service, brightness) =
            brightness::BrightnessService::new(system_dbus.clone());
        let brightness_service = spawn_service(|cancel| brightness_service.run(cancel));

        let (clipboard_service, clipboard) = clipboard::ClipboardService::new();
        let clipboard_service = spawn_service(|cancel| clipboard_service.run(cancel));

        let (power_service, power) = power::PowerService::new(system_dbus.clone());
        let power_service = spawn_service(|cancel| power_service.run(cancel));

        let (bluetooth_service, bluetooth) = bluetooth::BluetoothService::new(system_dbus.clone());
        let bluetooth_service = spawn_service(|cancel| bluetooth_service.run(cancel));

        let (network_service, network) = network::NetworkService::new(system_dbus.clone());
        let network_service = spawn_service(|cancel| network_service.run(cancel));

        let (notifications_service, notifications) =
            notifications::NotificationsService::new(session_dbus.clone());
        let notifications_service = spawn_service(|cancel| notifications_service.run(cancel));

        let (session_service, session) = session::SessionService::new(system_dbus.clone());
        let session_service = spawn_service(|cancel| session_service.run(cancel));

        let (compositor_service, compositor) = compositor::CompositorService::new();
        let compositor_service = spawn_service(|cancel| compositor_service.run(cancel));

        let (keyboard_service, keyboard) = keyboard::KeyboardService::new(compositor.clone());
        let keyboard_service = spawn_service(|cancel| keyboard_service.run(cancel));

        let (weather_service, weather) = weather::WeatherService::new(location.clone());
        let weather_service = spawn_service(|cancel| weather_service.run(cancel));

        let (tray_service, tray) = tray::TrayService::new(session_dbus.clone());
        let tray_service = spawn_service(|cancel| tray_service.run(cancel));

        let (webcam_service, webcam) = webcam::WebcamService::new();
        let webcam_service = spawn_service(|cancel| webcam_service.run(cancel));

        let running_services = vec![
            audio_events_service,
            audio_service,
            clock_service,
            calendar_events_service,
            geoclue_service,
            location_service,
            microphone_service,
            mpris_service,
            battery_service,
            brightness_service,
            clipboard_service,
            power_service,
            bluetooth_service,
            network_service,
            notifications_service,
            session_service,
            compositor_service,
            keyboard_service,
            weather_service,
            tray_service,
            webcam_service,
        ];
        let handles = Services {
            audio,
            audio_events,
            clock,
            calendar_events,
            geoclue,
            location,
            microphone,
            mpris,
            battery,
            brightness,
            clipboard,
            power,
            bluetooth,
            network,
            notifications,
            session,
            compositor,
            keyboard,
            weather,
            tray,
            webcam,
            system_dbus,
            session_dbus,
        };
        Self {
            handles,
            running_services,
        }
    }

    pub fn handles(&self) -> Services {
        self.handles.clone()
    }

    pub fn broadcast(&self, control: Control) {
        self.handles.broadcast(control);
    }

    pub async fn shutdown(mut self) {
        self.broadcast(Control::Shutdown);
        for service in self.running_services.drain(..) {
            service.cancel().await;
        }
    }
}

fn spawn_service<F, Fut>(run: F) -> RunningService
where
    F: FnOnce(CancellationToken) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let task = tokio::spawn(async move { run(task_cancel).await });
    RunningService { cancel, task }
}
