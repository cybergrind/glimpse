use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::services::control::ControlEvent;

pub enum ServiceEvent<Command> {
    Command(Command),
    Control(ControlEvent),
}

pub enum ServiceInput<Cfg, Cmd, Evt> {
    Reconfigure(Cfg),
    Command(Cmd),
    Event(Evt),
    SourceStopped,
}

pub enum ServiceEffect<Cfg, Err> {
    None,
    RestartInputs,
    RestartInputsWith(Cfg),
    Stop,
    Degraded(Err),
}

pub trait Service: Send + 'static {
    type Config: Clone + PartialEq + Send + 'static;
    type State: Clone + Send + 'static;
    type Command: Send + 'static;
    type Event: Send + 'static;
    type Error: std::fmt::Debug + Send + 'static;

    fn initial_state(config: &Self::Config) -> Self::State;
    fn spawn_inputs(
        config: &Self::Config,
        input_tx: mpsc::Sender<ServiceInput<Self::Config, Self::Command, Self::Event>>,
        cancel: CancellationToken,
    ) -> Vec<tokio::task::JoinHandle<()>>;

    fn reduce(
        state: &mut Self::State,
        input: ServiceInput<Self::Config, Self::Command, Self::Event>,
    ) -> ServiceEffect<Self::Config, Self::Error>;
}

#[derive(Clone)]
pub struct ServiceHandle<State, Command> {
    state_rx: watch::Receiver<State>,
    command_tx: mpsc::Sender<ServiceEvent<Command>>,
}

impl<State: Clone, Command: Send + 'static> ServiceHandle<State, Command> {
    pub fn snapshot(&self) -> State {
        self.state_rx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<State> {
        self.state_rx.clone()
    }

    pub async fn send(
        &self,
        command: ServiceEvent<Command>,
    ) -> Result<(), mpsc::error::SendError<ServiceEvent<Command>>> {
        self.command_tx.send(command).await
    }
}

pub struct RunningService<Command> {
    pub name: &'static str,
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
    event_tx: mpsc::Sender<ServiceEvent<Command>>,
}

impl<Command: Send + 'static> RunningService<Command> {
    pub fn try_send(
        &self,
        event: ServiceEvent<Command>,
    ) -> Result<(), mpsc::error::TrySendError<ServiceEvent<Command>>> {
        self.event_tx.try_send(event)
    }

    pub async fn send(
        &self,
        event: ServiceEvent<Command>,
    ) -> Result<(), mpsc::error::SendError<ServiceEvent<Command>>> {
        self.event_tx.send(event).await
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }
}

pub fn spawn_service<Command, F, Fut, Err>(
    service_name: &'static str,
    run: F,
) -> RunningService<Command>
where
    Command: Send + 'static,
    F: FnOnce(CancellationToken, mpsc::Receiver<ServiceEvent<Command>>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), Err>> + Send + 'static,
    Err: std::fmt::Debug + Send + 'static,
{
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    let (event_tx, event_rx) = mpsc::channel(16);

    let task = tokio::spawn(async move {
        if let Err(error) = run(task_cancel, event_rx).await {
            tracing::warn!(error = ?error, "{service_name} service stopped with error");
        }
    });

    RunningService {
        task,
        cancel,
        event_tx,
        name: service_name,
    }
}

pub struct RunningTask<Command> {
    pub task: JoinHandle<()>,
    pub control_tx: mpsc::Sender<Command>,
    pub cancel: CancellationToken,
}

impl<Command: Send + 'static> RunningTask<Command> {
    pub async fn cancel(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }

    pub async fn send(&self, command: Command) -> Result<(), mpsc::error::SendError<Command>> {
        self.control_tx.send(command).await
    }

    pub fn try_send(&self, command: Command) -> Result<(), mpsc::error::TrySendError<Command>> {
        self.control_tx.try_send(command)
    }
}
