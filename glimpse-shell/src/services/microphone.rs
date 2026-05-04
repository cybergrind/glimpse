use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::mpsc as std_mpsc,
    thread,
    time::Instant,
};

use libpulse_binding as pulse;
use pulse::{
    callbacks::ListResult,
    context::{
        Context as PulseContext, FlagSet as PulseContextFlagSet, State as PulseContextState,
        subscribe::{Facility, InterestMaskSet, Operation},
    },
    mainloop::standard::{IterateResult as PulseIterateResult, Mainloop as PulseMainloop},
    operation::State as PulseOperationState,
    proplist::Proplist as PulseProplist,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, watch},
    time::{Duration, sleep},
};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

const COMMAND_QUEUE_SIZE: usize = 4;
const RETRY_DELAY: Duration = Duration::from_secs(5);
const PULSE_QUERY_TIMEOUT: Duration = Duration::from_secs(2);
const PULSE_ITERATION_PAUSE: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicrophoneUsage {
    pub index: u64,
    pub app_name: String,
    pub app_icon: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub available: bool,
    pub usages: Vec<MicrophoneUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh,
}

pub type MicrophoneHandle = ServiceHandle<State, Command>;

pub struct MicrophoneService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

enum MonitorControl {
    Refresh,
    Shutdown,
}

enum MonitorMessage {
    State(State),
    Failed(String),
}

impl MicrophoneService {
    pub fn new() -> (Self, MicrophoneHandle) {
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
                    tracing::warn!(%error, "microphone service failed");
                    RunOutcome::RetryAfterDelay
                }
            };

            match outcome {
                RunOutcome::Cancelled => break,
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
        let (monitor_tx, mut monitor_rx) = mpsc::unbounded_channel();
        let (control_tx, control_rx) = std_mpsc::channel();
        let monitor = thread::Builder::new()
            .name("glimpse-microphone-pulse".into())
            .spawn(move || {
                if let Err(error) = run_pulse_monitor(monitor_tx.clone(), control_rx) {
                    let _ = monitor_tx.send(MonitorMessage::Failed(error.to_string()));
                }
            })?;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    stop_monitor(control_tx, monitor).await;
                    return Ok(RunOutcome::Cancelled);
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        stop_monitor(control_tx, monitor).await;
                        return Ok(RunOutcome::Cancelled);
                    }
                    Some(ServiceCommand::Control(Control::Start(_) | Control::Reconfigure(_)))
                    | Some(ServiceCommand::Command(Command::Refresh)) => {
                        let _ = control_tx.send(MonitorControl::Refresh);
                    }
                },
                message = monitor_rx.recv() => match message {
                    Some(MonitorMessage::State(state)) => self.change_state(state),
                    Some(MonitorMessage::Failed(error)) => {
                        tracing::warn!(%error, "pulse microphone monitor failed");
                        self.change_state(State::default());
                        stop_monitor(control_tx, monitor).await;
                        return Ok(RunOutcome::RetryAfterDelay);
                    }
                    None => {
                        self.change_state(State::default());
                        stop_monitor(control_tx, monitor).await;
                        return Ok(RunOutcome::RetryAfterDelay);
                    }
                }
            }
        }
    }

    fn change_state(&self, state: State) {
        if *self.state_tx.borrow() == state {
            return;
        }

        if let Err(error) = self.state_tx.send(state) {
            tracing::error!(?error, "failed to publish microphone state");
        }
    }
}

async fn stop_monitor(
    control_tx: std_mpsc::Sender<MonitorControl>,
    monitor: thread::JoinHandle<()>,
) {
    let _ = control_tx.send(MonitorControl::Shutdown);
    let _ = tokio::task::spawn_blocking(move || monitor.join()).await;
}

fn run_pulse_monitor(
    state_tx: mpsc::UnboundedSender<MonitorMessage>,
    control_rx: std_mpsc::Receiver<MonitorControl>,
) -> anyhow::Result<()> {
    let mut proplist =
        PulseProplist::new().ok_or_else(|| anyhow::anyhow!("failed to create pulse proplist"))?;
    proplist
        .set_str(
            pulse::proplist::properties::APPLICATION_NAME,
            "glimpse-shell",
        )
        .map_err(|()| anyhow::anyhow!("failed to set pulse application name"))?;

    let mut mainloop =
        PulseMainloop::new().ok_or_else(|| anyhow::anyhow!("failed to create pulse mainloop"))?;
    let mut context =
        PulseContext::new_with_proplist(&mainloop, "glimpse-shell-microphone-monitor", &proplist)
            .ok_or_else(|| anyhow::anyhow!("failed to create pulse context"))?;
    context
        .connect(None, PulseContextFlagSet::NOFLAGS, None)
        .map_err(|error| anyhow::anyhow!("failed to connect to pulse: {error:?}"))?;
    wait_for_pulse_context(
        &mut mainloop,
        &context,
        Instant::now() + PULSE_QUERY_TIMEOUT,
    )?;

    let pending_refresh = Rc::new(Cell::new(false));
    let pending_refresh_ref = Rc::clone(&pending_refresh);
    context.set_subscribe_callback(Some(Box::new(move |facility, operation, _index| {
        if facility == Some(Facility::SourceOutput)
            && matches!(
                operation,
                Some(Operation::New | Operation::Changed | Operation::Removed)
            )
        {
            pending_refresh_ref.set(true);
        }
    })));
    let subscribe = context.subscribe(InterestMaskSet::SOURCE_OUTPUT, |_| {});
    wait_for_pulse_operation(
        &mut mainloop,
        &subscribe,
        Instant::now() + PULSE_QUERY_TIMEOUT,
    )?;

    publish_usages(&mut mainloop, &context, &state_tx)?;

    loop {
        match control_rx.try_recv() {
            Ok(MonitorControl::Refresh) => pending_refresh.set(true),
            Ok(MonitorControl::Shutdown) => break,
            Err(std_mpsc::TryRecvError::Empty) => {}
            Err(std_mpsc::TryRecvError::Disconnected) => break,
        }

        iterate_pulse(&mut mainloop)?;

        if pending_refresh.replace(false) {
            publish_usages(&mut mainloop, &context, &state_tx)?;
        }
    }

    context.disconnect();
    Ok(())
}

fn publish_usages(
    mainloop: &mut PulseMainloop,
    context: &PulseContext,
    state_tx: &mpsc::UnboundedSender<MonitorMessage>,
) -> anyhow::Result<()> {
    state_tx
        .send(MonitorMessage::State(State {
            available: true,
            usages: fetch_microphone_usages(mainloop, context)?,
        }))
        .map_err(|error| anyhow::anyhow!("failed to publish microphone state: {error}"))
}

fn fetch_microphone_usages(
    mainloop: &mut PulseMainloop,
    context: &PulseContext,
) -> anyhow::Result<Vec<MicrophoneUsage>> {
    let done = Rc::new(Cell::new(false));
    let usages = Rc::new(RefCell::new(Vec::new()));
    let failed = Rc::new(Cell::new(false));
    let done_ref = Rc::clone(&done);
    let usages_ref = Rc::clone(&usages);
    let failed_ref = Rc::clone(&failed);

    let operation = context
        .introspect()
        .get_source_output_info_list(move |result| match result {
            ListResult::Item(info) => {
                if let Some(usage) = microphone_usage_from_pulse_info(
                    u64::from(info.index),
                    |key| info.proplist.get_str(key),
                    info.name.as_deref(),
                ) {
                    usages_ref.borrow_mut().push(usage);
                }
            }
            ListResult::End => done_ref.set(true),
            ListResult::Error => {
                failed_ref.set(true);
                done_ref.set(true);
            }
        });

    wait_for_pulse_operation(mainloop, &operation, Instant::now() + PULSE_QUERY_TIMEOUT)?;

    if failed.get() {
        return Err(anyhow::anyhow!("pulse source output query failed"));
    }

    let mut usages = usages.borrow().clone();
    usages.sort_by(|left, right| {
        (left.app_name.as_str(), left.index).cmp(&(right.app_name.as_str(), right.index))
    });
    Ok(usages)
}

fn wait_for_pulse_context(
    mainloop: &mut PulseMainloop,
    context: &PulseContext,
    deadline: Instant,
) -> anyhow::Result<()> {
    loop {
        if Instant::now() >= deadline {
            return Err(anyhow::anyhow!("pulse context connection timed out"));
        }
        iterate_pulse(mainloop)?;

        match context.get_state() {
            PulseContextState::Ready => return Ok(()),
            PulseContextState::Failed | PulseContextState::Terminated => {
                return Err(anyhow::anyhow!("pulse context failed"));
            }
            _ => {}
        }
    }
}

fn wait_for_pulse_operation<T: ?Sized>(
    mainloop: &mut PulseMainloop,
    operation: &pulse::operation::Operation<T>,
    deadline: Instant,
) -> anyhow::Result<()> {
    while operation.get_state() == PulseOperationState::Running {
        if Instant::now() >= deadline {
            return Err(anyhow::anyhow!("pulse operation timed out"));
        }
        iterate_pulse(mainloop)?;
    }

    match operation.get_state() {
        PulseOperationState::Done => Ok(()),
        PulseOperationState::Cancelled => Err(anyhow::anyhow!("pulse operation cancelled")),
        PulseOperationState::Running => unreachable!("running pulse operation loop exited"),
    }
}

fn iterate_pulse(mainloop: &mut PulseMainloop) -> anyhow::Result<()> {
    match mainloop.iterate(false) {
        PulseIterateResult::Success(_) => {
            thread::sleep(PULSE_ITERATION_PAUSE);
            Ok(())
        }
        PulseIterateResult::Quit(retval) => Err(anyhow::anyhow!("pulse mainloop quit: {retval:?}")),
        PulseIterateResult::Err(error) => Err(anyhow::anyhow!("pulse mainloop error: {error:?}")),
    }
}

fn microphone_usage_from_pulse_info(
    index: u64,
    prop: impl Fn(&str) -> Option<String>,
    stream_name: Option<&str>,
) -> Option<MicrophoneUsage> {
    let app_name =
        first_non_empty_string(&[prop("application.name"), stream_name.map(ToOwned::to_owned)])
            .unwrap_or_else(|| "Unknown".into());

    if is_ignored_microphone_client(prop("application.id").as_deref(), &app_name) {
        return None;
    }

    Some(MicrophoneUsage {
        index,
        app_name,
        app_icon: first_non_empty_string(&[prop("application.icon_name")])
            .unwrap_or_else(|| "application-x-executable-symbolic".into()),
    })
}

fn is_ignored_microphone_client(app_id: Option<&str>, app_name: &str) -> bool {
    const IGNORED_APP_IDS: &[&str] = &["org.gnome.VolumeControl", "org.PulseAudio.pavucontrol"];

    app_id.is_some_and(|id| IGNORED_APP_IDS.contains(&id))
        || matches!(app_name, "PulseAudio Volume Control" | "Volume Control")
}

fn first_non_empty_string(items: &[Option<String>]) -> Option<String> {
    items
        .iter()
        .flatten()
        .find(|item| !item.is_empty())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_recording_apps_from_pulse_source_outputs() {
        let usage = microphone_usage_from_pulse_info(
            7,
            |key| match key {
                "application.name" => Some("Telegram".into()),
                "application.icon_name" => Some("telegram".into()),
                _ => None,
            },
            None,
        );

        assert_eq!(
            usage,
            Some(MicrophoneUsage {
                index: 7,
                app_name: "Telegram".into(),
                app_icon: "telegram".into(),
            })
        );
    }

    #[test]
    fn ignores_volume_control_source_outputs() {
        assert_eq!(
            microphone_usage_from_pulse_info(
                8,
                |key| match key {
                    "application.id" => Some("org.PulseAudio.pavucontrol".into()),
                    "application.name" => Some("PulseAudio Volume Control".into()),
                    _ => None,
                },
                None,
            ),
            None
        );
    }
}
