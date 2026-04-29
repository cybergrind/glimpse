use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    config::Config,
    dbus::Dbus,
    services::{battery, bluetooth, compositor, location, network, power, session},
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
                    "failed to broadcast control: {}", stringify!(err)
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
    pub location: ServiceHandle<location::State, location::Command>,
    pub battery: ServiceHandle<battery::State, battery::Command>,
    pub power: ServiceHandle<power::State, power::Command>,
    pub bluetooth: ServiceHandle<bluetooth::State, bluetooth::Command>,
    pub network: network::NetworkHandle,
    pub session: session::SessionHandle,
    pub compositor: compositor::CompositorHandle,
    pub system_dbus: zbus::Connection,
    pub session_dbus: zbus::Connection,
}

impl Services {
    pub fn broadcast(&self, control: Control) {
        for_each_service_handle!(
            self,
            control,
            [
                location, battery, power, bluetooth, network, session, compositor
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

        let (location_service, location) = location::LocationService::new();
        let location_service = spawn_service(|cancel| location_service.run(cancel));

        let (battery_service, battery) = battery::BatteryService::new(system_dbus.clone());
        let battery_service = spawn_service(|cancel| battery_service.run(cancel));

        let (power_service, power) = power::PowerService::new(system_dbus.clone());
        let power_service = spawn_service(|cancel| power_service.run(cancel));

        let (bluetooth_service, bluetooth) = bluetooth::BluetoothService::new(system_dbus.clone());
        let bluetooth_service = spawn_service(|cancel| bluetooth_service.run(cancel));

        let (network_service, network) = network::NetworkService::new(system_dbus.clone());
        let network_service = spawn_service(|cancel| network_service.run(cancel));

        let (session_service, session) = session::SessionService::new(system_dbus.clone());
        let session_service = spawn_service(|cancel| session_service.run(cancel));

        let (compositor_service, compositor) = compositor::CompositorService::new();
        let compositor_service = spawn_service(|cancel| compositor_service.run(cancel));

        let running_services = vec![
            location_service,
            battery_service,
            power_service,
            bluetooth_service,
            network_service,
            session_service,
            compositor_service,
        ];
        let handles = Services {
            location,
            battery,
            power,
            bluetooth,
            network,
            session,
            compositor,
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
