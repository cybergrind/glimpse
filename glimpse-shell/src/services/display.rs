use adw::gdk::{self, prelude::DisplayExt, prelude::MonitorExt};
use gio::prelude::ListModelExt;
use glib::object::{CastNone, ObjectExt};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

#[derive(Clone, Debug)]
pub enum Command {}

#[derive(Clone, Debug)]
pub struct Monitor {
    pub connector: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale_factor: i32,
    pub refresh_rate_mhz: i32,
    pub model: Option<String>,
    pub manufacturer: Option<String>,
}

impl Monitor {
    fn from_gdk(monitor: &gdk::Monitor) -> Self {
        let geom = monitor.geometry();
        Self {
            connector: monitor.connector().map(|s| s.to_string()),
            x: geom.x(),
            y: geom.y(),
            width: geom.width(),
            height: geom.height(),
            scale_factor: monitor.scale_factor(),
            refresh_rate_mhz: monitor.refresh_rate(),
            model: monitor.model().map(|s| s.to_string()),
            manufacturer: monitor.manufacturer().map(|s| s.to_string()),
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct State {
    pub monitors: Vec<Monitor>,
}

impl State {
    pub fn monitor_by_connector(&self, connector: &str) -> Option<&Monitor> {
        self.monitors
            .iter()
            .find(|m| m.connector.as_deref() == Some(connector))
    }
}

pub struct DisplayService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

impl DisplayService {
    pub fn new() -> (Self, ServiceHandle<State, Command>) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(4);

        (
            Self {
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    fn change_state(&self, state: State) {
        if let Err(err) = self.state_tx.send(state) {
            tracing::error!("failed to send new state: {:?}", err);
        }
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        tracing::debug!("display service started");

        let (watch_tx, mut watch_rx) = mpsc::channel::<WatchMessage>(8);
        let mut watcher: Option<CancellationToken> = None;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(message) = watch_rx.recv() => match message {
                    WatchMessage::MonitorsChanged(monitors) => {
                        tracing::debug!(count = monitors.len(), "monitors snapshot");
                        self.change_state(State { monitors });
                    }
                    WatchMessage::Error(err) => {
                        tracing::error!("display watcher error: {err}");
                    }
                },
                Some(command) = self.command_rx.recv() => match command {
                    ServiceCommand::Control(Control::Start(_)) => {
                        if let Some(token) = watcher.take() {
                            token.cancel();
                        }
                        let local_cancel = CancellationToken::new();
                        watcher = Some(local_cancel.clone());
                        let watch_tx = watch_tx.clone();
                        let task_cancel = local_cancel.clone();
                        // hop onto the main thread before spawn_local so the
                        // TaskSource's thread_guard binds to the iterating
                        // thread (GTK main), not this tokio worker.
                        glib::MainContext::default().invoke(move || {
                            glib::MainContext::default().spawn_local(async move {
                                watch_displays(watch_tx, task_cancel).await;
                            });
                        });
                        tracing::debug!("display watcher started");
                    }
                    ServiceCommand::Control(Control::Reconfigure(_)) => {}
                    ServiceCommand::Control(Control::Shutdown) => {
                        tracing::debug!("display service shutdown received");
                        break;
                    }
                    ServiceCommand::Command(cmd) => match cmd {},
                },
            }
        }

        if let Some(token) = watcher.take() {
            token.cancel();
        }
        tracing::debug!("display service quit");
    }
}

enum WatchMessage {
    MonitorsChanged(Vec<Monitor>),
    Error(String),
}

/// Must be spawned on the GTK main thread (e.g. via `relm4::spawn_local`),
/// because `gdk::Display::default()` and GDK signals are main-thread-only.
async fn watch_displays(value_sender: mpsc::Sender<WatchMessage>, cancel: CancellationToken) {
    let Some(display) = gdk::Display::default() else {
        value_sender
            .send(WatchMessage::Error("no default display".to_string()))
            .await
            .ok();
        return;
    };

    let monitors = display.monitors();

    if value_sender
        .send(WatchMessage::MonitorsChanged(collect_monitors(&monitors)))
        .await
        .is_err()
    {
        return;
    }

    let callback_sender = value_sender.clone();
    let handler_id = monitors.connect_items_changed(move |model, pos, removed, added| {
        tracing::debug!(pos, removed, added, "monitors changed");
        let snapshot = collect_monitors(model);
        if let Err(err) = callback_sender.try_send(WatchMessage::MonitorsChanged(snapshot)) {
            tracing::warn!("dropped monitors-changed event: {err}");
        }
    });

    cancel.cancelled().await;
    monitors.disconnect(handler_id);
}

fn collect_monitors(model: &gio::ListModel) -> Vec<Monitor> {
    (0..model.n_items())
        .filter_map(|i| model.item(i).and_downcast::<gdk::Monitor>())
        .map(|m| Monitor::from_gdk(&m))
        .collect()
}

/// Look up a live `gdk::Monitor` by its connector name. Must be called on the
/// GTK main thread (GObjects are thread-affine). Use for wiring panels to a
/// specific output via `gtk4_layer_shell::set_monitor`.
pub fn gdk_monitor_by_connector(connector: &str) -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    let monitors = display.monitors();
    (0..monitors.n_items())
        .filter_map(|i| monitors.item(i).and_downcast::<gdk::Monitor>())
        .find(|m| m.connector().map(|c| c == connector).unwrap_or(false))
}
