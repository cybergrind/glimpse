use std::collections::HashMap;

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    Config, KeyboardConfig, KeyboardRememberMode,
    compositors::{self, keyboard_layout_code},
    services::{
        compositor,
        framework::{Control, ServiceCommand, ServiceHandle},
    },
};

const COMMAND_QUEUE_SIZE: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardLayout {
    pub index: usize,
    pub name: String,
    pub code: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub available: bool,
    pub layouts: Vec<KeyboardLayout>,
    pub current_layout: Option<KeyboardLayout>,
    pub current_index: Option<usize>,
    pub remember: KeyboardRememberMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetLayout(usize),
    NextLayout,
    PreviousLayout,
}

pub type KeyboardHandle = ServiceHandle<State, Command>;

pub struct KeyboardService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    compositor: compositor::CompositorHandle,
    config: KeyboardConfig,
    remember: RememberState,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum KeyboardScope {
    App(String),
    Window(usize),
}

#[derive(Debug, Default)]
struct RememberState {
    focused_scope: Option<KeyboardScope>,
    remembered: HashMap<KeyboardScope, usize>,
}

impl KeyboardService {
    pub fn new(compositor: compositor::CompositorHandle) -> (Self, KeyboardHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                state_tx,
                command_rx,
                compositor,
                config: KeyboardConfig::default(),
                remember: RememberState::default(),
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        let mut compositor_rx = self.compositor.subscribe();
        let compositor_state = compositor_rx.borrow().clone();
        self.apply_compositor_state(&compositor_state).await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = compositor_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let compositor_state = compositor_rx.borrow().clone();
                    self.apply_compositor_state(&compositor_state).await;
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => self.execute_command(command).await,
                    Some(ServiceCommand::Control(control)) => match control {
                        Control::Start(config) | Control::Reconfigure(config) => {
                            self.apply_config(&config).await;
                        }
                        Control::Shutdown => break,
                    },
                    None => break,
                }
            }
        }
    }

    async fn apply_config(&mut self, config: &Config) {
        let config = KeyboardConfig::from_config(config);
        if self.config != config {
            self.config = config;
            let compositor_state = self.compositor.snapshot();
            self.apply_compositor_state(&compositor_state).await;
        }
    }

    async fn apply_compositor_state(&mut self, compositor: &compositor::State) {
        if let Some(target) = self.remember_target(compositor) {
            self.set_layout(target).await;
        }

        let state = state_from_compositor(compositor, &self.config);
        self.state_tx
            .send_if_modified(|current| set_if_changed(current, state));
    }

    fn remember_target(&mut self, state: &compositor::State) -> Option<usize> {
        let current = state.current_keyboard_layout;
        let scope = focused_scope(self.config.remember, state);

        if self.remember.focused_scope == scope {
            return None;
        }

        if let (Some(previous), Some(current)) = (self.remember.focused_scope.take(), current) {
            self.remember.remembered.insert(previous, current);
        }
        self.remember.focused_scope = scope.clone();

        scope
            .and_then(|scope| self.remember.remembered.get(&scope).copied())
            .filter(|target| Some(*target) != current)
            .filter(|target| {
                state
                    .keyboard_layouts
                    .iter()
                    .any(|layout| layout.index == *target)
            })
    }

    async fn execute_command(&mut self, command: Command) {
        let state = self.state_tx.borrow().clone();
        let target = match command {
            Command::SetLayout(index) => state
                .layouts
                .iter()
                .any(|layout| layout.index == index)
                .then_some(index),
            Command::NextLayout => next_layout_index(state.current_index, &state.layouts, true),
            Command::PreviousLayout => {
                next_layout_index(state.current_index, &state.layouts, false)
            }
        };

        if let Some(target) = target {
            self.set_layout(target).await;
        }
    }

    async fn set_layout(&self, index: usize) {
        if let Err(error) = self
            .compositor
            .send(ServiceCommand::Command(
                compositor::Command::SetKeyboardLayout(index),
            ))
            .await
        {
            tracing::warn!(%error, index, "failed to send keyboard layout command");
        }
    }
}

fn state_from_compositor(state: &compositor::State, config: &KeyboardConfig) -> State {
    let layouts = normalize_layouts(&state.keyboard_layouts, &config.labels);
    let current_layout = state
        .current_keyboard_layout
        .and_then(|index| layouts.iter().find(|layout| layout.index == index))
        .cloned();
    let current_index = current_layout.as_ref().map(|layout| layout.index);

    State {
        available: state.capabilities.keyboard_layouts && !layouts.is_empty(),
        layouts,
        current_layout,
        current_index,
        remember: config.remember,
    }
}

fn normalize_layouts(
    layouts: &[compositors::KeyboardLayout],
    labels: &HashMap<String, String>,
) -> Vec<KeyboardLayout> {
    layouts
        .iter()
        .map(|layout| {
            let code = keyboard_layout_code(&layout.name);
            let label = layout_label(&layout.name, &code, labels);
            KeyboardLayout {
                index: layout.index,
                name: layout.name.clone(),
                code,
                label,
            }
        })
        .collect()
}

fn layout_label(name: &str, code: &str, labels: &HashMap<String, String>) -> String {
    labels
        .get(name)
        .or_else(|| labels.get(&name.to_lowercase()))
        .or_else(|| labels.get(&code.to_lowercase()))
        .or_else(|| labels.get(code))
        .cloned()
        .unwrap_or_else(|| code.to_owned())
}

fn next_layout_index(
    current: Option<usize>,
    layouts: &[KeyboardLayout],
    next: bool,
) -> Option<usize> {
    if layouts.len() < 2 {
        return None;
    }

    let current = current?;
    let position = layouts.iter().position(|layout| layout.index == current)?;
    let target = if next {
        (position + 1) % layouts.len()
    } else {
        (position + layouts.len() - 1) % layouts.len()
    };

    Some(layouts[target].index)
}

fn focused_scope(mode: KeyboardRememberMode, state: &compositor::State) -> Option<KeyboardScope> {
    match mode {
        KeyboardRememberMode::Global => None,
        KeyboardRememberMode::Window => state.focused_window.map(KeyboardScope::Window),
        KeyboardRememberMode::App => state
            .focused_window
            .and_then(|id| state.windows.iter().find(|window| window.id == id))
            .and_then(|window| window.app_id.as_ref())
            .filter(|app_id| !app_id.is_empty())
            .cloned()
            .map(KeyboardScope::App),
    }
}

fn set_if_changed<T: PartialEq>(target: &mut T, value: T) -> bool {
    if *target == value {
        false
    } else {
        *target = value;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositors::{CompositorCapabilities, Window};

    #[test]
    fn normalize_layouts_resolves_label_overrides() {
        let layouts = vec![
            raw_layout(0, "English (US)"),
            raw_layout(1, "pl"),
            raw_layout(2, "German"),
        ];
        let labels = HashMap::from([
            ("English (US)".into(), "🇺🇸".into()),
            ("pl".into(), "PL!".into()),
            ("de".into(), "DE!".into()),
        ]);

        let layouts = normalize_layouts(&layouts, &labels);

        assert_eq!(layouts[0].code, "EN");
        assert_eq!(layouts[0].label, "🇺🇸");
        assert_eq!(layouts[1].label, "PL!");
        assert_eq!(layouts[2].label, "DE!");
    }

    #[test]
    fn state_from_compositor_publishes_normalized_current_layout() {
        let compositor = compositor::State {
            capabilities: CompositorCapabilities {
                keyboard_layouts: true,
                ..CompositorCapabilities::default()
            },
            keyboard_layouts: vec![raw_layout(0, "us"), raw_layout(1, "pl")],
            current_keyboard_layout: Some(1),
            ..compositor::State::default()
        };

        let state = state_from_compositor(&compositor, &KeyboardConfig::default());

        assert!(state.available);
        assert_eq!(state.current_index, Some(1));
        assert_eq!(state.current_layout.unwrap().label, "PL");
    }

    #[test]
    fn next_layout_index_uses_actual_layout_indices() {
        let layouts = vec![layout(2, "US"), layout(4, "PL"), layout(7, "DE")];

        assert_eq!(next_layout_index(Some(2), &layouts, true), Some(4));
        assert_eq!(next_layout_index(Some(7), &layouts, true), Some(2));
        assert_eq!(next_layout_index(Some(2), &layouts, false), Some(7));
        assert_eq!(next_layout_index(Some(1), &layouts, true), None);
        assert_eq!(next_layout_index(Some(2), &layouts[..1], true), None);
    }

    #[test]
    fn remember_state_stores_previous_app_layout_and_restores_next_app() {
        let mut service = service_with_remember(KeyboardRememberMode::App);
        let mut state = compositor_state(Some(1), Some(10), "editor");

        assert_eq!(service.remember_target(&state), None);

        state.current_keyboard_layout = Some(2);
        state.focused_window = Some(20);
        state.windows = vec![window(20, "browser")];
        assert_eq!(service.remember_target(&state), None);

        state.current_keyboard_layout = Some(0);
        state.focused_window = Some(10);
        state.windows = vec![window(10, "editor")];
        assert_eq!(service.remember_target(&state), Some(2));
    }

    #[test]
    fn remember_state_stores_previous_window_layout_and_restores_next_window() {
        let mut service = service_with_remember(KeyboardRememberMode::Window);
        let mut state = compositor_state(Some(1), Some(10), "editor");

        assert_eq!(service.remember_target(&state), None);

        state.current_keyboard_layout = Some(2);
        state.focused_window = Some(20);
        state.windows = vec![window(20, "editor")];
        assert_eq!(service.remember_target(&state), None);

        state.current_keyboard_layout = Some(0);
        state.focused_window = Some(10);
        state.windows = vec![window(10, "editor")];
        assert_eq!(service.remember_target(&state), Some(2));
    }

    #[test]
    fn global_remember_mode_never_targets_restore() {
        let mut service = service_with_remember(KeyboardRememberMode::Global);
        let state = compositor_state(Some(1), Some(10), "editor");

        assert_eq!(service.remember_target(&state), None);
        assert_eq!(service.remember.focused_scope, None);
    }

    fn service_with_remember(mode: KeyboardRememberMode) -> KeyboardService {
        let (compositor_service, compositor) = compositor::CompositorService::new();
        drop(compositor_service);
        let (mut service, _handle) = KeyboardService::new(compositor);
        service.config.remember = mode;
        service
    }

    fn compositor_state(
        current_keyboard_layout: Option<usize>,
        focused_window: Option<usize>,
        app_id: &str,
    ) -> compositor::State {
        compositor::State {
            capabilities: CompositorCapabilities {
                keyboard_layouts: true,
                ..CompositorCapabilities::default()
            },
            keyboard_layouts: vec![
                raw_layout(0, "us"),
                raw_layout(1, "pl"),
                raw_layout(2, "de"),
            ],
            current_keyboard_layout,
            focused_window,
            windows: focused_window
                .map(|id| vec![window(id, app_id)])
                .unwrap_or_default(),
            ..compositor::State::default()
        }
    }

    fn raw_layout(index: usize, name: &str) -> compositors::KeyboardLayout {
        compositors::KeyboardLayout {
            index,
            name: name.into(),
        }
    }

    fn layout(index: usize, label: &str) -> KeyboardLayout {
        KeyboardLayout {
            index,
            name: label.into(),
            code: label.into(),
            label: label.into(),
        }
    }

    fn window(id: usize, app_id: &str) -> Window {
        Window {
            id,
            title: None,
            app_id: Some(app_id.into()),
            pid: None,
            layout_order: None,
            workspace: None,
            focused: true,
            urgent: false,
            fullscreen: false,
            floating: None,
        }
    }
}
