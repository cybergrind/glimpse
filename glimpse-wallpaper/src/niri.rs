//! Niri IPC client for workspace change events.
//!
//! Connects to the niri Unix socket, subscribes to the event stream, and
//! invokes a callback whenever the active workspace changes on any output.
//!
//! The [`start_workspace_watcher`] function is a no-op when not running under
//! niri — it checks [`glimpse::compositor::detect`] before connecting.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use serde::Deserialize;
use tracing::{debug, info, warn};

// ── IPC types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct Workspace {
    pub id: u64,
    pub idx: u8,
    pub output: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
enum NiriEvent {
    WorkspacesChanged { workspaces: Vec<Workspace> },
    WorkspaceActivated { id: u64, #[allow(dead_code)] focused: bool },
    #[serde(other)]
    Other,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Spawns a background thread that listens to niri IPC events.
///
/// `on_change` is called with `(output_connector, workspace_index)` whenever
/// the active workspace changes on any output. It is invoked from the watcher
/// thread, so it must be `Send + 'static`. Typically this is a closure that
/// calls `sender.input(AppMsg::WorkspaceChanged { .. })`.
///
/// Returns `None` if not running under niri, `NIRI_SOCKET` is not set,
/// or the socket cannot be opened.
pub fn start_workspace_watcher(
    on_change: impl Fn(String, u8) + Send + 'static,
) -> Option<()> {
    if !glimpse::compositor::supports_niri_ipc() {
        return None;
    }

    let socket_path = std::env::var("NIRI_SOCKET").ok()?;

    let stream = match UnixStream::connect(&socket_path) {
        Ok(s) => s,
        Err(e) => {
            warn!("niri: cannot connect to {socket_path}: {e}");
            return None;
        }
    };

    info!("niri: connected to {socket_path}");

    std::thread::spawn(move || {
        run_event_loop(stream, on_change);
    });

    Some(())
}

// ── Event loop ────────────────────────────────────────────────────────────────

fn run_event_loop(
    mut stream: UnixStream,
    on_change: impl Fn(String, u8),
) {
    if let Err(e) = writeln!(stream, "\"EventStream\"") {
        warn!("niri: failed to send EventStream request: {e}");
        return;
    }

    let reader = BufReader::new(stream);
    let mut workspace_map: HashMap<u64, Workspace> = HashMap::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                warn!("niri: IPC read error: {e}");
                break;
            }
        };

        if line.contains("\"Handled\"") || line.contains("\"Ok\"") {
            debug!("niri: subscription acknowledged");
            continue;
        }

        let event: NiriEvent = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                debug!("niri: ignoring unparseable event ({e}): {line}");
                continue;
            }
        };

        match event {
            NiriEvent::WorkspacesChanged { workspaces } => {
                workspace_map.clear();

                for ws in &workspaces {
                    if ws.is_active {
                        if let Some(ref output) = ws.output {
                            on_change(output.clone(), ws.idx);
                        }
                    }
                }

                for ws in workspaces {
                    workspace_map.insert(ws.id, ws);
                }

                debug!("niri: workspaces refreshed");
            }

            NiriEvent::WorkspaceActivated { id, .. } => {
                if let Some(ws) = workspace_map.get(&id) {
                    if let Some(ref output) = ws.output {
                        info!("niri: workspace {} activated on {output}", ws.idx);
                        on_change(output.clone(), ws.idx);
                    }
                }
            }

            NiriEvent::Other => {}
        }
    }

    warn!("niri: IPC event stream ended");
}
