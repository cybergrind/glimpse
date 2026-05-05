use std::fs;

use tokio::sync::{mpsc, watch};
use tokio::time::{Duration, Instant, MissedTickBehavior, interval, sleep};
use tokio_util::sync::CancellationToken;

use crate::{
    compositors::{
        Compositor, CompositorCapabilities, CompositorEvent, CompositorRefresh, CompositorSnapshot,
        CompositorStructureSnapshot, CompositorType, KeyboardLayout, KeyboardLayoutSnapshot,
        Monitor, ScreencastKind, ScreencastSession, ScreencastTarget, Window, Workspace,
        detect_compositor,
    },
    services::framework::{Control, ServiceCommand, ServiceHandle},
};

const COMMAND_QUEUE_SIZE: usize = 8;
const EVENT_QUEUE_SIZE: usize = 32;
const RETRY_DELAY: Duration = Duration::from_secs(2);
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(40);
const DIRECT_SCREENCAST_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const DIRECT_SCREENCAST_ID_PREFIX: &str = "direct-screen-capture:";
const PORTAL_SCREENCAST_ID_PREFIX: &str = "portal-screen-capture:";
const PORTAL_DESKTOP_DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PORTAL_SESSION_ROOT: &str = "/org/freedesktop/portal/desktop/session";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub compositor: CompositorType,
    pub capabilities: CompositorCapabilities,
    pub windows: Vec<Window>,
    pub workspaces: Vec<Workspace>,
    pub monitors: Vec<Monitor>,
    pub screencasts: Vec<ScreencastSession>,
    pub current_keyboard_layout: Option<usize>,
    pub focused_window: Option<usize>,
    pub current_workspace: Option<usize>,
    pub keyboard_layouts: Vec<KeyboardLayout>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Command {
    SetKeyboardLayout(usize),
    SetWorkspace(usize),
    FocusNextWorkspace,
    FocusPreviousWorkspace,
    FocusWindow(usize),
    FocusNextWindow,
    FocusPreviousWindow,
    StopScreencast(String),
}

pub type CompositorHandle = ServiceHandle<State, Command>;

pub struct CompositorService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

impl CompositorService {
    pub fn new() -> (Self, CompositorHandle) {
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
                    tracing::warn!(error = %error, "compositor service failed");
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
        let Some(compositor) = detect_compositor() else {
            self.replace_state(State::default());
            tracing::warn!("compositor service: unsupported compositor");
            return Ok(RunOutcome::RetryAfterDelay);
        };

        tracing::info!(
            compositor = compositor.name(),
            "compositor service: connected"
        );
        self.publish_identity(compositor);
        self.refresh_snapshot(compositor).await;

        let (event_tx, mut event_rx) = mpsc::channel(EVENT_QUEUE_SIZE);
        let listener = tokio::spawn(compositor.listen(event_tx));
        tokio::pin!(listener);
        let refresh_timer = sleep(REFRESH_DEBOUNCE);
        tokio::pin!(refresh_timer);
        let mut pending_refresh = None;
        let mut direct_screencast_tick = interval(DIRECT_SCREENCAST_REFRESH_INTERVAL);
        direct_screencast_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
        direct_screencast_tick.tick().await;
        self.refresh_external_screencasts().await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    listener.abort();
                    return Ok(RunOutcome::Cancelled);
                }
                result = &mut listener => {
                    match result {
                        Ok(Ok(())) => tracing::warn!("compositor event listener stopped"),
                        Ok(Err(error)) => tracing::warn!(error = %error, "compositor event listener failed"),
                        Err(error) if error.is_cancelled() => {}
                        Err(error) => tracing::warn!(error = %error, "compositor event listener task failed"),
                    }
                    return Ok(RunOutcome::RetryAfterDelay);
                }
                _ = &mut refresh_timer, if pending_refresh.is_some() => {
                    if let Some(refresh) = pending_refresh.take() {
                        self.refresh(compositor, refresh).await;
                    }
                }
                event = event_rx.recv() => match event {
                    Some(CompositorEvent::RefreshRequested(refresh)) => {
                        let schedule_refresh = pending_refresh.is_none();
                        pending_refresh = Some(match pending_refresh {
                            Some(pending) => pending.merge(refresh),
                            None => refresh,
                        });
                        if schedule_refresh {
                            refresh_timer.as_mut().reset(Instant::now() + REFRESH_DEBOUNCE);
                        }
                    }
                    Some(event) => self.apply_event(compositor, event).await,
                    None => return Ok(RunOutcome::RetryAfterDelay),
                },
                _ = direct_screencast_tick.tick() => {
                    self.refresh_external_screencasts().await;
                },
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => {
                        self.execute_command(compositor, command).await;
                    }
                    Some(ServiceCommand::Control(control)) => match control {
                        Control::Start(_) | Control::Reconfigure(_) => {}
                        Control::Shutdown => {
                            listener.abort();
                            return Ok(RunOutcome::Cancelled);
                        }
                    },
                    None => {
                        listener.abort();
                        return Ok(RunOutcome::Cancelled);
                    }
                },
            }
        }
    }

    async fn apply_event(&self, compositor: Compositor, event: CompositorEvent) {
        let compositor_type = compositor.compositor_type();
        self.state_tx.send_if_modified(|state| {
            if event.name() != "window-changed" {
                tracing::debug!(
                    compositor = compositor.name(),
                    event = event.name(),
                    "compositor event"
                );
            }
            let mut changed = set_if_changed(&mut state.compositor, compositor_type);
            match event {
                CompositorEvent::Snapshot(snapshot) => {
                    changed |= apply_snapshot(state, compositor_type, snapshot);
                }
                CompositorEvent::RefreshRequested(_) => {}
                CompositorEvent::WindowsChanged(windows) => {
                    changed |= set_if_changed(&mut state.windows, windows);
                    changed |= sync_focused_window_from_windows(state);
                    changed |= sync_current_workspace_from_focus_or_workspace(state);
                }
                CompositorEvent::WindowChanged(window) => {
                    changed |= apply_window_changed(state, window);
                }
                CompositorEvent::WindowTitleChanged { window, title } => {
                    if let Some(item) = state.windows.iter_mut().find(|item| item.id == window) {
                        changed |= set_if_changed(&mut item.title, Some(title));
                    }
                }
                CompositorEvent::WindowFullscreenChanged { window, fullscreen } => {
                    if let Some(window) = window.or(state.focused_window) {
                        if let Some(item) = state.windows.iter_mut().find(|item| item.id == window)
                        {
                            changed |= set_if_changed(&mut item.fullscreen, fullscreen);
                        }
                    }
                }
                CompositorEvent::WindowFloatingChanged { window, floating } => {
                    if let Some(item) = state.windows.iter_mut().find(|item| item.id == window) {
                        changed |= set_if_changed(&mut item.floating, Some(floating));
                    }
                }
                CompositorEvent::WindowClosed(window) => {
                    let len = state.windows.len();
                    state.windows.retain(|item| item.id != window);
                    changed |= state.windows.len() != len;
                    if state.focused_window == Some(window) {
                        changed |= set_if_changed(&mut state.focused_window, None);
                    }
                    changed |= mark_focused_window(&mut state.windows, state.focused_window);
                }
                CompositorEvent::WorkspacesChanged(workspaces) => {
                    changed |= set_if_changed(&mut state.workspaces, workspaces);
                    let current_workspace = state
                        .workspaces
                        .iter()
                        .find(|workspace| workspace.focused)
                        .map(|workspace| workspace.id);
                    changed |= set_if_changed(&mut state.current_workspace, current_workspace);
                }
                CompositorEvent::WorkspaceChanged { id, focused } => {
                    if focused {
                        changed |= set_if_changed(&mut state.current_workspace, Some(id));
                    }
                    for workspace in &mut state.workspaces {
                        changed |=
                            set_if_changed(&mut workspace.focused, workspace.id == id && focused);
                        if workspace.id == id {
                            changed |= set_if_changed(&mut workspace.active, true);
                        }
                    }
                }
                CompositorEvent::WorkspaceActiveWindowChanged { workspace, window } => {
                    if let Some(item) = state
                        .workspaces
                        .iter_mut()
                        .find(|item| item.id == workspace)
                    {
                        changed |= set_if_changed(&mut item.active_window, window);
                    }
                    if state.current_workspace == Some(workspace) {
                        changed |= set_if_changed(&mut state.focused_window, window);
                    }
                    changed |= mark_focused_window(&mut state.windows, state.focused_window);
                }
                CompositorEvent::MonitorsChanged(monitors) => {
                    changed |= set_if_changed(&mut state.monitors, monitors);
                    let current_workspace = state
                        .monitors
                        .iter()
                        .find(|monitor| monitor.focused)
                        .and_then(|monitor| monitor.active_workspace)
                        .or_else(|| {
                            state
                                .workspaces
                                .iter()
                                .find(|workspace| workspace.focused)
                                .map(|workspace| workspace.id)
                        });
                    changed |= set_if_changed(&mut state.current_workspace, current_workspace);
                }
                CompositorEvent::MonitorChanged {
                    name,
                    active_workspace,
                    focused,
                } => {
                    for monitor in &mut state.monitors {
                        if focused {
                            changed |= set_if_changed(&mut monitor.focused, monitor.name == name);
                        }
                        if monitor.name == name {
                            changed |=
                                set_if_changed(&mut monitor.active_workspace, active_workspace);
                        }
                    }
                    if focused {
                        changed |= set_if_changed(&mut state.current_workspace, active_workspace);
                    }
                }
                CompositorEvent::KeyboardLayoutsChanged { layouts, current } => {
                    changed |= set_if_changed(&mut state.keyboard_layouts, layouts);
                    changed |= set_if_changed(&mut state.current_keyboard_layout, current);
                }
                CompositorEvent::KeyboardLayoutChanged { index, name } => {
                    let current = index.or_else(|| {
                        name.as_deref().and_then(|name| {
                            state
                                .keyboard_layouts
                                .iter()
                                .position(|layout| layout.name == name)
                        })
                    });
                    if current.is_some() || index.is_some() {
                        changed |= set_if_changed(&mut state.current_keyboard_layout, current);
                    }
                }
                CompositorEvent::FocusedWindowChanged(window) => {
                    changed |= set_if_changed(&mut state.focused_window, window);
                    changed |= mark_focused_window(&mut state.windows, state.focused_window);
                    changed |= sync_current_workspace_from_focus_or_workspace(state);
                }
                CompositorEvent::ScreencastsChanged(screencasts) => {
                    changed |= apply_compositor_screencasts(&mut state.screencasts, screencasts);
                }
                CompositorEvent::ScreencastChanged(screencast) => {
                    changed |= apply_screencast_changed(state, screencast);
                }
                CompositorEvent::ScreencastStopped(id) => {
                    let len = state.screencasts.len();
                    state.screencasts.retain(|item| item.id != id);
                    changed |= state.screencasts.len() != len;
                }
            }

            changed
        });
    }

    async fn refresh(&self, compositor: Compositor, refresh: CompositorRefresh) {
        if refresh.is_full() {
            self.refresh_snapshot(compositor).await;
            return;
        }

        if refresh.includes_structure() {
            self.refresh_structure(compositor).await;
        }
        if refresh.includes_keyboard_layouts() {
            self.refresh_keyboard_layouts(compositor).await;
        }
    }

    async fn refresh_snapshot(&self, compositor: Compositor) {
        match compositor.snapshot().await {
            Ok(snapshot) => {
                let compositor_type = compositor.compositor_type();
                self.state_tx
                    .send_if_modified(|state| apply_snapshot(state, compositor_type, snapshot));
            }
            Err(error) => {
                tracing::warn!(error = %error, "failed to refresh compositor snapshot");
                self.state_tx.send_if_modified(|state| {
                    set_if_changed(&mut state.compositor, compositor.compositor_type())
                });
            }
        }
    }

    fn publish_identity(&self, compositor: Compositor) {
        let compositor_type = compositor.compositor_type();
        let capabilities = compositor.capabilities();
        self.state_tx.send_if_modified(|state| {
            let mut changed = set_if_changed(&mut state.compositor, compositor_type);
            changed |= set_if_changed(&mut state.capabilities, capabilities);
            changed
        });
    }

    async fn refresh_structure(&self, compositor: Compositor) {
        match compositor.structure_snapshot().await {
            Ok(snapshot) => {
                let compositor_type = compositor.compositor_type();
                self.state_tx.send_if_modified(|state| {
                    let mut changed = set_if_changed(&mut state.compositor, compositor_type);
                    changed |= apply_structure_snapshot(state, snapshot);
                    changed
                });
            }
            Err(error) => {
                tracing::warn!(error = %error, "failed to refresh compositor structure");
                self.refresh_snapshot(compositor).await;
            }
        }
    }

    async fn refresh_keyboard_layouts(&self, compositor: Compositor) {
        match compositor.keyboard_layout_snapshot().await {
            Ok(snapshot) => {
                let compositor_type = compositor.compositor_type();
                self.state_tx.send_if_modified(|state| {
                    let mut changed = set_if_changed(&mut state.compositor, compositor_type);
                    changed |= apply_keyboard_layout_snapshot(state, snapshot);
                    changed
                });
            }
            Err(error) => {
                tracing::warn!(error = %error, "failed to refresh compositor keyboard layouts");
                self.refresh_snapshot(compositor).await;
            }
        }
    }

    fn replace_state(&self, state: State) {
        self.state_tx
            .send_if_modified(|current| set_if_changed(current, state));
    }

    async fn execute_command(&self, compositor: Compositor, command: Command) {
        let result = match command {
            Command::SetKeyboardLayout(layout) => compositor.set_keyboard_layout(layout).await,
            Command::SetWorkspace(workspace) => compositor.set_workspace(workspace).await,
            Command::FocusNextWorkspace => compositor.focus_next_workspace().await,
            Command::FocusPreviousWorkspace => compositor.focus_previous_workspace().await,
            Command::FocusWindow(window) => compositor.focus_window(window).await,
            Command::FocusNextWindow => compositor.focus_next_window().await,
            Command::FocusPreviousWindow => compositor.focus_previous_window().await,
            Command::StopScreencast(session_id) => compositor.stop_screencast(&session_id).await,
        };

        if let Err(error) = result {
            tracing::warn!(error = %error, "compositor command failed");
        }
    }

    async fn refresh_external_screencasts(&self) {
        let mut screencasts = match tokio::task::spawn_blocking(scan_direct_screencasts).await {
            Ok(screencasts) => screencasts,
            Err(error) => {
                tracing::warn!(?error, "failed to scan direct screencast usage");
                Vec::new()
            }
        };

        match scan_portal_screencasts().await {
            Ok(portal_screencasts) => screencasts.extend(portal_screencasts),
            Err(error) => {
                tracing::debug!(%error, "failed to scan portal screencast usage");
            }
        }

        self.state_tx.send_if_modified(|state| {
            apply_direct_screencasts(&mut state.screencasts, screencasts)
        });
    }
}

fn apply_snapshot(
    state: &mut State,
    compositor: CompositorType,
    snapshot: CompositorSnapshot,
) -> bool {
    let CompositorSnapshot {
        capabilities,
        windows,
        workspaces,
        monitors,
        screencasts,
        keyboard_layouts,
        current_keyboard_layout,
        focused_window,
        current_workspace,
    } = snapshot;
    let mut changed = set_if_changed(&mut state.compositor, compositor);
    changed |= set_if_changed(&mut state.capabilities, capabilities);
    changed |= apply_compositor_screencasts(&mut state.screencasts, screencasts);
    changed |= apply_structure_snapshot(
        state,
        CompositorStructureSnapshot {
            windows,
            workspaces,
            monitors,
            focused_window,
            current_workspace,
        },
    );
    changed |= apply_keyboard_layout_snapshot(
        state,
        KeyboardLayoutSnapshot {
            keyboard_layouts,
            current_keyboard_layout,
        },
    );
    changed
}

fn apply_screencast_changed(state: &mut State, screencast: ScreencastSession) -> bool {
    if !screencast.active {
        let len = state.screencasts.len();
        state.screencasts.retain(|item| item.id != screencast.id);
        return state.screencasts.len() != len;
    }

    if let Some(existing) = state
        .screencasts
        .iter_mut()
        .find(|item| item.id == screencast.id)
    {
        return set_if_changed(existing, screencast);
    }

    state.screencasts.push(screencast);
    true
}

fn apply_compositor_screencasts(
    screencasts: &mut Vec<ScreencastSession>,
    compositor: Vec<ScreencastSession>,
) -> bool {
    let original = screencasts.clone();
    screencasts.retain(|session| is_direct_screencast(&session.id));
    screencasts.extend(compositor);
    screencasts.sort_by(|left, right| left.id.cmp(&right.id));
    *screencasts != original
}

fn apply_direct_screencasts(
    screencasts: &mut Vec<ScreencastSession>,
    direct: Vec<ScreencastSession>,
) -> bool {
    let original = screencasts.clone();
    screencasts.retain(|session| !is_direct_screencast(&session.id));
    screencasts.extend(direct);
    screencasts.sort_by(|left, right| left.id.cmp(&right.id));
    *screencasts != original
}

fn apply_structure_snapshot(state: &mut State, snapshot: CompositorStructureSnapshot) -> bool {
    let mut changed = set_if_changed(&mut state.windows, snapshot.windows);
    changed |= set_if_changed(&mut state.workspaces, snapshot.workspaces);
    changed |= set_if_changed(&mut state.monitors, snapshot.monitors);
    changed |= set_if_changed(&mut state.focused_window, snapshot.focused_window);
    changed |= set_if_changed(&mut state.current_workspace, snapshot.current_workspace);
    changed
}

fn apply_keyboard_layout_snapshot(state: &mut State, snapshot: KeyboardLayoutSnapshot) -> bool {
    let mut changed = set_if_changed(&mut state.keyboard_layouts, snapshot.keyboard_layouts);
    changed |= set_if_changed(
        &mut state.current_keyboard_layout,
        snapshot.current_keyboard_layout,
    );
    changed
}

fn apply_window_changed(state: &mut State, window: Window) -> bool {
    let focused = window.focused;
    let workspace = window.workspace;
    let window_id = window.id;
    let was_focused = state.focused_window == Some(window_id);
    let mut changed = upsert_by_id(&mut state.windows, window, |window| window.id);

    if focused {
        changed |= set_if_changed(&mut state.focused_window, Some(window_id));
        changed |= mark_focused_window(&mut state.windows, state.focused_window);
        if let Some(workspace) = workspace {
            changed |= set_if_changed(&mut state.current_workspace, Some(workspace));
        }
    } else if was_focused {
        changed |= mark_focused_window(&mut state.windows, state.focused_window);
        changed |= sync_current_workspace_from_focus_or_workspace(state);
    }

    changed
}

fn set_if_changed<T>(slot: &mut T, value: T) -> bool
where
    T: PartialEq,
{
    if *slot == value {
        false
    } else {
        *slot = value;
        true
    }
}

fn upsert_by_id<T, F>(items: &mut Vec<T>, item: T, id: F) -> bool
where
    T: PartialEq,
    F: Fn(&T) -> usize,
{
    let item_id = id(&item);
    match items.iter().position(|existing| id(existing) == item_id) {
        Some(index) if items[index] != item => {
            items[index] = item;
            true
        }
        Some(_) => false,
        None => {
            items.push(item);
            true
        }
    }
}

fn mark_focused_window(windows: &mut [Window], focused_window: Option<usize>) -> bool {
    let mut changed = false;
    for window in windows {
        changed |= set_if_changed(&mut window.focused, Some(window.id) == focused_window);
    }
    changed
}

fn sync_focused_window_from_windows(state: &mut State) -> bool {
    let focused_window = state
        .windows
        .iter()
        .find(|window| window.focused)
        .map(|window| window.id);
    set_if_changed(&mut state.focused_window, focused_window)
}

fn sync_current_workspace_from_focus_or_workspace(state: &mut State) -> bool {
    let current_workspace = state
        .focused_window
        .and_then(|focused| state.windows.iter().find(|window| window.id == focused))
        .and_then(|window| window.workspace)
        .or_else(|| {
            state
                .workspaces
                .iter()
                .find(|workspace| workspace.focused)
                .map(|workspace| workspace.id)
        });
    set_if_changed(&mut state.current_workspace, current_workspace)
}

fn scan_direct_screencasts() -> Vec<ScreencastSession> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };

    let mut screencasts = entries
        .flatten()
        .filter_map(|entry| {
            let pid = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<i32>().ok())?;
            let process_name = process_name(pid)?;
            if !is_direct_screencast_process(&process_name) {
                return None;
            }

            Some(ScreencastSession {
                id: format!("{DIRECT_SCREENCAST_ID_PREFIX}{pid}:{process_name}"),
                session_id: None,
                kind: ScreencastKind::WlrScreencopy,
                target: ScreencastTarget::Monitor,
                active: true,
                pipewire_node: None,
                client_pid: Some(pid),
                stoppable: false,
            })
        })
        .collect::<Vec<_>>();

    screencasts.sort_by(|left, right| left.id.cmp(&right.id));
    screencasts
}

fn process_name(pid: i32) -> Option<String> {
    let name = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

fn is_direct_screencast_process(name: &str) -> bool {
    matches!(
        name,
        "wf-recorder"
            | "wl-screenrec"
            | "gpu-screen-recorder"
            | "kooha"
            | "obs"
            | "obs-studio"
            | "wl-mirror"
    )
}

fn is_direct_screencast(id: &str) -> bool {
    id.starts_with(DIRECT_SCREENCAST_ID_PREFIX) || id.starts_with(PORTAL_SCREENCAST_ID_PREFIX)
}

async fn scan_portal_screencasts() -> anyhow::Result<Vec<ScreencastSession>> {
    let connection = zbus::Connection::session().await?;
    let mut sessions = Vec::new();
    for path in portal_session_leaf_paths(&connection).await? {
        let Some(name) = path.rsplit('/').next() else {
            continue;
        };
        if !is_portal_screencast_session_name(name) {
            continue;
        }
        sessions.push(ScreencastSession {
            id: format!("{PORTAL_SCREENCAST_ID_PREFIX}{name}"),
            session_id: Some(path),
            kind: ScreencastKind::PipeWire,
            target: ScreencastTarget::Unknown,
            active: true,
            pipewire_node: None,
            client_pid: None,
            stoppable: false,
        });
    }
    sessions.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(sessions)
}

async fn portal_session_leaf_paths(connection: &zbus::Connection) -> anyhow::Result<Vec<String>> {
    let mut leaves = Vec::new();
    let top_level = introspect_child_node_names(connection, PORTAL_SESSION_ROOT).await?;
    for account in top_level {
        let account_path = format!("{PORTAL_SESSION_ROOT}/{account}");
        let children = introspect_child_node_names(connection, &account_path).await?;
        for child in children {
            leaves.push(format!("{account_path}/{child}"));
        }
    }
    Ok(leaves)
}

async fn introspect_child_node_names(
    connection: &zbus::Connection,
    path: &str,
) -> anyhow::Result<Vec<String>> {
    let proxy = zbus::fdo::IntrospectableProxy::builder(connection)
        .destination(PORTAL_DESKTOP_DESTINATION)?
        .path(path)?
        .build()
        .await?;
    Ok(child_node_names(&proxy.introspect().await?))
}

fn child_node_names(xml: &str) -> Vec<String> {
    xml.match_indices("<node ")
        .filter_map(|(index, _)| {
            let rest = &xml[index..];
            let name = rest.split_once("name=\"")?.1.split_once('"')?.0;
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

fn is_portal_screencast_session_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("webrtc") || name.contains("screencast") || name.contains("screen_cast")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositors::{ScreencastKind, ScreencastTarget, niri::Niri};

    #[test]
    fn publishes_compositor_identity_before_snapshot_data() {
        let (service, handle) = CompositorService::new();

        service.publish_identity(Compositor::Niri(Niri));

        let state = handle.snapshot();
        assert_eq!(state.compositor, CompositorType::Niri);
        assert!(state.capabilities.workspaces);
        assert!(state.capabilities.windows);
    }

    #[test]
    fn applies_structure_snapshot_only_updates_structure_state() {
        let mut state = State {
            compositor: CompositorType::Hyprland,
            capabilities: CompositorCapabilities {
                windows: true,
                workspaces: true,
                monitors: true,
                keyboard_layouts: true,
                focused_window: true,
                current_workspace: true,
                fullscreen: true,
                floating: true,
                window_titles: true,
                night_light: true,
                screencast_state: crate::compositors::ScreencastStateCapability::None,
                screencast_control: crate::compositors::ScreencastControlCapability::None,
            },
            ..State::default()
        };
        let snapshot = CompositorStructureSnapshot {
            windows: vec![window(1, true, Some(3))],
            workspaces: vec![workspace(3, true)],
            monitors: vec![monitor("DP-1", Some(3), true)],
            focused_window: Some(1),
            current_workspace: Some(3),
        };

        assert!(apply_structure_snapshot(&mut state, snapshot.clone()));
        assert!(state.capabilities.night_light);
        assert!(!apply_structure_snapshot(&mut state, snapshot));
    }

    #[test]
    fn focus_helpers_only_report_real_changes() {
        let mut state = State {
            windows: vec![window(1, false, Some(1)), window(2, false, Some(4))],
            workspaces: vec![workspace(4, true)],
            focused_window: Some(2),
            current_workspace: Some(1),
            ..State::default()
        };

        assert!(mark_focused_window(
            &mut state.windows,
            state.focused_window
        ));
        assert!(!mark_focused_window(
            &mut state.windows,
            state.focused_window
        ));
        assert!(sync_current_workspace_from_focus_or_workspace(&mut state));
        assert_eq!(state.current_workspace, Some(4));
        assert!(!sync_current_workspace_from_focus_or_workspace(&mut state));
    }

    #[test]
    fn current_focused_window_update_preserves_focus_marker() {
        let mut state = State {
            windows: vec![window(7, true, Some(1))],
            focused_window: Some(7),
            current_workspace: Some(1),
            ..State::default()
        };
        let update = window(7, false, Some(2));

        assert!(apply_window_changed(&mut state, update));
        assert_eq!(state.focused_window, Some(7));
        assert_eq!(state.current_workspace, Some(2));
        assert!(state.windows[0].focused);
    }

    #[test]
    fn set_if_changed_suppresses_noop_updates() {
        let mut value = Some(1);

        assert!(!set_if_changed(&mut value, Some(1)));
        assert!(set_if_changed(&mut value, Some(2)));
        assert_eq!(value, Some(2));
    }

    #[test]
    fn applies_direct_screencasts_without_removing_compositor_sessions() {
        let mut screencasts = vec![screencast("niri:1", ScreencastKind::PipeWire)];
        let direct = vec![screencast(
            "direct-screen-capture:42:wf-recorder",
            ScreencastKind::WlrScreencopy,
        )];

        assert!(apply_direct_screencasts(&mut screencasts, direct));
        assert_eq!(screencasts.len(), 2);

        assert!(!apply_direct_screencasts(
            &mut screencasts,
            vec![screencast(
                "direct-screen-capture:42:wf-recorder",
                ScreencastKind::WlrScreencopy,
            )],
        ));

        assert!(apply_direct_screencasts(&mut screencasts, Vec::new()));
        assert_eq!(
            screencasts,
            vec![screencast("niri:1", ScreencastKind::PipeWire)]
        );
    }

    #[test]
    fn compositor_screencast_replacement_preserves_direct_sessions() {
        let mut screencasts = vec![
            screencast("old-niri", ScreencastKind::PipeWire),
            screencast(
                "direct-screen-capture:42:wf-recorder",
                ScreencastKind::WlrScreencopy,
            ),
        ];

        assert!(apply_compositor_screencasts(
            &mut screencasts,
            vec![screencast("new-niri", ScreencastKind::PipeWire)]
        ));

        assert_eq!(
            screencasts,
            vec![
                screencast(
                    "direct-screen-capture:42:wf-recorder",
                    ScreencastKind::WlrScreencopy,
                ),
                screencast("new-niri", ScreencastKind::PipeWire),
            ]
        );
    }

    #[test]
    fn identifies_direct_screencast_processes() {
        assert!(is_direct_screencast_process("wf-recorder"));
        assert!(is_direct_screencast_process("wl-screenrec"));
        assert!(is_direct_screencast_process("obs"));
        assert!(!is_direct_screencast_process("firefox"));
        assert!(!is_direct_screencast_process("pipewire"));
    }

    #[test]
    fn parses_portal_session_child_nodes() {
        let xml = r#"
            <node>
              <interface name="org.freedesktop.DBus.Introspectable"/>
              <node name="1_803571"/>
              <node name="1_803688"/>
            </node>
        "#;

        assert_eq!(
            child_node_names(xml),
            vec!["1_803571".to_string(), "1_803688".to_string()]
        );
    }

    #[test]
    fn identifies_portal_webrtc_screen_sessions() {
        assert!(is_portal_screencast_session_name(
            "webrtc_session1227734951"
        ));
        assert!(is_portal_screencast_session_name("screen_cast123"));
        assert!(!is_portal_screencast_session_name("gtk1810709476"));
        assert!(!is_portal_screencast_session_name("tdesktop2568826995"));
    }

    fn window(id: usize, focused: bool, workspace: Option<usize>) -> Window {
        Window {
            id,
            title: None,
            app_id: None,
            pid: None,
            layout_order: None,
            workspace,
            focused,
            urgent: false,
            fullscreen: false,
            floating: None,
        }
    }

    fn workspace(id: usize, focused: bool) -> Workspace {
        Workspace {
            id,
            index: Some(id),
            name: None,
            monitor: None,
            active: focused,
            focused,
            urgent: false,
            active_window: None,
        }
    }

    fn monitor(name: &str, active_workspace: Option<usize>, focused: bool) -> Monitor {
        Monitor {
            id: None,
            name: name.into(),
            description: None,
            active_workspace,
            focused,
        }
    }

    fn screencast(id: &str, kind: ScreencastKind) -> ScreencastSession {
        ScreencastSession {
            id: id.into(),
            session_id: None,
            kind,
            target: ScreencastTarget::Monitor,
            active: true,
            pipewire_node: None,
            client_pid: None,
            stoppable: false,
        }
    }
}
