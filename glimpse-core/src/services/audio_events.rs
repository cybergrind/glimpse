use std::process::Stdio;

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command as TokioCommand,
    sync::{mpsc, watch},
    time::{Duration, sleep},
};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

const COMMAND_QUEUE_SIZE: usize = 2;
const RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub available: bool,
    pub sequence: u64,
    pub event: Option<Event>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Sink,
    Source,
    SourceOutput,
    SinkInput,
    Server,
}

#[derive(Debug, Clone)]
pub enum Command {}

pub type AudioEventsHandle = ServiceHandle<State, Command>;

pub struct AudioEventsService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

enum RunOutcome {
    Cancelled,
    Restart,
    RetryAfterDelay,
}

impl AudioEventsService {
    pub fn new() -> (Self, AudioEventsHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        loop {
            let outcome = match self.run_inner(cancel.clone()).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    tracing::warn!(%error, "audio events service failed");
                    self.publish_unavailable();
                    RunOutcome::RetryAfterDelay
                }
            };

            match outcome {
                RunOutcome::Cancelled => break,
                RunOutcome::Restart => continue,
                RunOutcome::RetryAfterDelay => {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = sleep(RETRY_DELAY) => {}
                    }
                }
            }
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<RunOutcome> {
        let mut sub = TokioCommand::new("pactl")
            .arg("subscribe")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        self.publish_available();

        let stdout = sub
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("pactl subscribe did not expose stdout"))?;
        let mut lines = BufReader::new(stdout).lines();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = sub.kill().await;
                    return Ok(RunOutcome::Cancelled);
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        let _ = sub.kill().await;
                        return Ok(RunOutcome::Cancelled);
                    }
                    Some(ServiceCommand::Control(Control::Start(_) | Control::Reconfigure(_))) => {}
                    Some(ServiceCommand::Command(command)) => match command {},
                },
                line = lines.next_line() => match line {
                    Ok(Some(line)) => {
                        if let Some(event) = event_from_line(&line) {
                            self.publish_event(event);
                        }
                    }
                    Ok(None) => {
                        self.publish_unavailable();
                        let _ = sub.kill().await;
                        return Ok(RunOutcome::Restart);
                    }
                    Err(error) => {
                        self.publish_unavailable();
                        let _ = sub.kill().await;
                        return Err(error.into());
                    }
                }
            }
        }
    }

    fn publish_available(&self) {
        self.state_tx.send_if_modified(|state| {
            let changed = !state.available;
            state.available = true;
            changed
        });
    }

    fn publish_unavailable(&self) {
        self.state_tx.send_if_modified(|state| {
            let changed = state.available || state.event.is_some();
            state.available = false;
            state.event = None;
            changed
        });
    }

    fn publish_event(&self, event: Event) {
        self.state_tx.send_if_modified(|state| {
            state.available = true;
            state.sequence = state.sequence.wrapping_add(1);
            state.event = Some(event);
            true
        });
    }
}

pub fn event_from_line(line: &str) -> Option<Event> {
    let line = line.to_ascii_lowercase();
    if line.contains("source-output") || line.contains("source output") {
        Some(Event::SourceOutput)
    } else if line.contains("sink-input") || line.contains("sink input") {
        Some(Event::SinkInput)
    } else if line.contains("sink") {
        Some(Event::Sink)
    } else if line.contains("source") {
        Some(Event::Source)
    } else if line.contains("server") {
        Some(Event::Server)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pactl_subscribe_lines() {
        assert_eq!(
            event_from_line("Event 'new' on source-output #12"),
            Some(Event::SourceOutput)
        );
        assert_eq!(
            event_from_line("Event 'change' on sink-input #3"),
            Some(Event::SinkInput)
        );
        assert_eq!(
            event_from_line("Event 'change' on sink #1"),
            Some(Event::Sink)
        );
        assert_eq!(
            event_from_line("Event 'change' on source #1"),
            Some(Event::Source)
        );
        assert_eq!(
            event_from_line("Event 'change' on server"),
            Some(Event::Server)
        );
        assert_eq!(event_from_line("Event 'new' on card #1"), None);
    }
}
