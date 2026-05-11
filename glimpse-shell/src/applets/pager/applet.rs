#![allow(unused_assignments)]

use std::collections::HashSet;

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    compositors::{CompositorType, Monitor, Window, Workspace},
    panels::applets::AppletConfig,
    services::{
        compositor::{Command, CompositorHandle, State},
        framework::ServiceCommand,
    },
};

use super::components::strip::{
    Input as StripInput, Output as StripOutput, PagerAppearance, PagerItem, PagerTarget, Strip,
    View,
};

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
    Workspaces,
    #[default]
    Windows,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    display: DisplayMode,
    appearance: PagerAppearance,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        let settings = settings_without_legacy_style(raw);
        match settings.try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid pager applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            display: DisplayMode::Windows,
            appearance: PagerAppearance::Dots,
        }
    }
}

pub struct Applet {
    config: Config,
    state: PagerState,
    view: View,
    service: CompositorHandle,
    strip: Controller<Strip>,
    subscription_cancel: CancellationToken,
    panel_monitor: Option<String>,
}

#[derive(Debug)]
pub struct Init {
    pub service: CompositorHandle,
    pub config: Config,
    pub panel_monitor: Option<String>,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    StripOutput(StripOutput),
    Scroll { next: bool, horizontal: bool },
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        install_scroll_controller(&root, &sender);

        let strip = Strip::builder()
            .launch(())
            .forward(sender.input_sender(), Input::StripOutput);
        let strip_widget = strip.widget().clone();
        let mut state = PagerState::from(&init.service.snapshot());
        state.panel_monitor = init.panel_monitor.clone();
        let view = view_from_state(&init.config, &state);

        let model = Applet {
            config: init.config,
            state,
            view,
            service: init.service,
            strip,
            subscription_cancel: CancellationToken::new(),
            panel_monitor: init.panel_monitor,
        };

        let service = model.service.clone();
        let cancel = model.subscription_cancel.clone();
        let subscription_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            if subscription_sender
                .send(Input::ServiceStateChanged(sub.borrow().clone()))
                .is_err()
            {
                return;
            }

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() {
                            break;
                        }

                        if subscription_sender
                            .send(Input::ServiceStateChanged(sub.borrow().clone()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        let widgets = view_output!();
        widgets.root.append(&strip_widget);
        model.render();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                let mut state = PagerState::from(&state);
                state.panel_monitor = self.panel_monitor.clone();
                if self.state != state {
                    self.state = state;
                    self.sync_view();
                }
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.sync_view();
            }
            Input::StripOutput(StripOutput::Activate(target)) => {
                if let Some(command) = command_for_target(target) {
                    self.send_command(command);
                }
            }
            Input::Scroll { next, horizontal } => {
                self.send_command(self.scroll_command(next, horizontal));
            }
        }
    }
}

impl Applet {
    fn render(&self) {
        self.strip.emit(StripInput::Render(self.view.clone()));
    }

    fn sync_view(&mut self) {
        let view = view_from_state(&self.config, &self.state);
        if self.view != view {
            self.view = view;
            self.render();
        }
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send compositor command from pager applet");
            }
        });
    }

    fn scroll_command(&self, next: bool, horizontal: bool) -> Command {
        scroll_command(&self.config, &self.state, next, horizontal)
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PagerState {
    compositor: CompositorType,
    workspaces_available: bool,
    windows_available: bool,
    focused_window_available: bool,
    current_workspace: Option<usize>,
    focused_window: Option<usize>,
    workspaces: Vec<Workspace>,
    windows: Vec<PagerWindow>,
    monitors: Vec<Monitor>,
    panel_monitor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PagerWindow {
    id: usize,
    layout_order: Option<usize>,
    workspace: Option<usize>,
    focused: bool,
    urgent: bool,
}

impl From<&State> for PagerState {
    fn from(state: &State) -> Self {
        Self {
            compositor: state.compositor,
            workspaces_available: state.capabilities.workspaces,
            windows_available: state.capabilities.windows,
            focused_window_available: state.capabilities.focused_window,
            current_workspace: state.current_workspace,
            focused_window: state.focused_window,
            workspaces: state.workspaces.clone(),
            windows: state.windows.iter().map(PagerWindow::from).collect(),
            monitors: state.monitors.clone(),
            panel_monitor: None,
        }
    }
}

impl PagerState {
    fn effective_current_workspace(&self) -> Option<usize> {
        if let Some(name) = self.panel_monitor.as_deref() {
            if let Some(active) = self
                .monitors
                .iter()
                .find(|monitor| monitor.name == name)
                .and_then(|monitor| monitor.active_workspace)
            {
                return Some(active);
            }
        }
        self.current_workspace
    }

    fn is_panel_monitor_focused(&self) -> bool {
        let Some(name) = self.panel_monitor.as_deref() else {
            return true;
        };
        match self
            .monitors
            .iter()
            .find(|monitor| monitor.name == name)
        {
            Some(monitor) => monitor.focused,
            None => true,
        }
    }
}

impl From<&Window> for PagerWindow {
    fn from(window: &Window) -> Self {
        Self {
            id: window.id,
            layout_order: window.layout_order,
            workspace: window.workspace,
            focused: window.focused,
            urgent: window.urgent,
        }
    }
}

fn view_from_state(config: &Config, state: &PagerState) -> View {
    if !state.workspaces_available {
        return View {
            visible: false,
            tooltip: "Workspaces unavailable".into(),
            appearance: config.appearance,
            items: Vec::new(),
            placeholder: false,
        };
    }

    let panel_monitor = state.panel_monitor.as_deref();
    let current_workspace = state.effective_current_workspace();
    let monitor_focused = state.is_panel_monitor_focused();

    let (items, tooltip, placeholder) = match config.display {
        DisplayMode::Workspaces => (
            workspace_items(
                state.compositor,
                current_workspace,
                panel_monitor,
                monitor_focused,
                &state.workspaces,
                &state.windows,
                config.appearance,
            ),
            current_workspace_tooltip(state),
            false,
        ),
        DisplayMode::Windows => {
            let items = window_items(
                state.compositor,
                current_workspace,
                panel_monitor,
                monitor_focused,
                state.focused_window,
                &state.workspaces,
                &state.windows,
                config.appearance,
            );
            let placeholder = items.is_empty();
            (items, current_workspace_window_tooltip(state), placeholder)
        }
    };

    View {
        visible: true,
        tooltip,
        appearance: config.appearance,
        items,
        placeholder,
    }
}

fn settings_without_legacy_style(raw: &AppletConfig) -> toml::Value {
    let mut settings = raw.settings.clone();
    if let toml::Value::Table(table) = &mut settings {
        table.remove("style");
    }
    settings
}

fn workspace_items(
    compositor: CompositorType,
    current_workspace: Option<usize>,
    panel_monitor: Option<&str>,
    monitor_focused: bool,
    workspaces: &[Workspace],
    windows: &[PagerWindow],
    appearance: PagerAppearance,
) -> Vec<PagerItem> {
    let occupied = occupied_workspaces(windows);
    let urgent = urgent_workspaces(windows);
    let scoped_workspaces =
        scoped_workspaces(compositor, panel_monitor, current_workspace, workspaces);
    let current_slot = current_workspace_slot(compositor, current_workspace, &scoped_workspaces);
    let highest_window_workspace = match compositor {
        CompositorType::Niri => 0,
        CompositorType::Hyprland | CompositorType::Unsupported => occupied
            .iter()
            .chain(urgent.iter())
            .copied()
            .max()
            .unwrap_or(0),
    };
    let count = workspace_indicator_count_for_scope(
        compositor,
        current_slot,
        &scoped_workspaces,
        highest_window_workspace,
    );

    (1..=count)
        .map(|slot| {
            let workspace = workspace_for_slot(compositor, slot, &scoped_workspaces);
            let target = workspace_command_target(compositor, slot, workspace);
            PagerItem {
                id: target,
                target: PagerTarget::Workspace(target),
                appearance,
                label: slot.to_string(),
                focused: current_slot == Some(slot),
                monitor_focused,
                occupied: workspace
                    .and_then(|workspace| workspace.active_window)
                    .is_some()
                    || workspace
                        .map(|workspace| occupied.contains(&workspace.id))
                        .unwrap_or_else(|| occupied.contains(&slot)),
                urgent: workspace.map(|workspace| workspace.urgent).unwrap_or(false)
                    || workspace
                        .map(|workspace| urgent.contains(&workspace.id))
                        .unwrap_or_else(|| urgent.contains(&slot)),
            }
        })
        .collect()
}

fn window_items(
    compositor: CompositorType,
    current_workspace: Option<usize>,
    panel_monitor: Option<&str>,
    monitor_focused: bool,
    focused_window: Option<usize>,
    workspaces: &[Workspace],
    windows: &[PagerWindow],
    appearance: PagerAppearance,
) -> Vec<PagerItem> {
    let Some(current_workspace) = current_workspace else {
        return vec![window_placeholder_item(appearance, monitor_focused)];
    };

    if !workspace_belongs_to_panel(compositor, panel_monitor, current_workspace, workspaces) {
        return vec![window_placeholder_item(appearance, monitor_focused)];
    }

    let mut windows = windows
        .iter()
        .filter(|window| window.workspace == Some(current_workspace))
        .collect::<Vec<_>>();
    windows.sort_by_key(|window| (window.layout_order.unwrap_or(usize::MAX), window.id));

    let items = windows
        .into_iter()
        .enumerate()
        .map(|(index, window)| PagerItem {
            id: window.id,
            target: PagerTarget::Window(window.id),
            appearance,
            label: (index + 1).to_string(),
            focused: window.focused || focused_window == Some(window.id),
            monitor_focused,
            occupied: true,
            urgent: window.urgent,
        })
        .collect::<Vec<_>>();
    if items.is_empty() {
        vec![window_placeholder_item(appearance, monitor_focused)]
    } else {
        items
    }
}

fn window_placeholder_item(appearance: PagerAppearance, monitor_focused: bool) -> PagerItem {
    PagerItem {
        id: 0,
        target: PagerTarget::Placeholder,
        appearance,
        label: "1".into(),
        focused: true,
        monitor_focused,
        occupied: false,
        urgent: false,
    }
}

fn workspace_for_slot<'a>(
    compositor: CompositorType,
    slot: usize,
    workspaces: &[&'a Workspace],
) -> Option<&'a Workspace> {
    match compositor {
        CompositorType::Niri => workspaces
            .iter()
            .copied()
            .find(|workspace| workspace.index == Some(slot)),
        CompositorType::Hyprland | CompositorType::Unsupported => workspaces
            .iter()
            .copied()
            .find(|workspace| workspace.id == slot),
    }
}

fn workspace_command_target(
    compositor: CompositorType,
    fallback: usize,
    workspace: Option<&Workspace>,
) -> usize {
    match compositor {
        CompositorType::Niri => workspace
            .and_then(|workspace| workspace.index)
            .unwrap_or(fallback),
        CompositorType::Hyprland | CompositorType::Unsupported => {
            workspace.map(|workspace| workspace.id).unwrap_or(fallback)
        }
    }
}

#[cfg(test)]
fn workspace_indicator_count(
    compositor: CompositorType,
    current_workspace: Option<usize>,
    workspaces: &[Workspace],
    windows: &[PagerWindow],
) -> usize {
    let scoped_workspaces = scoped_workspaces(compositor, None, current_workspace, workspaces);
    let current_slot = current_workspace_slot(compositor, current_workspace, &scoped_workspaces);
    let highest_window_workspace = match compositor {
        CompositorType::Niri => 0,
        CompositorType::Hyprland | CompositorType::Unsupported => windows
            .iter()
            .filter_map(|window| window.workspace)
            .max()
            .unwrap_or(0),
    };
    workspace_indicator_count_for_scope(
        compositor,
        current_slot,
        &scoped_workspaces,
        highest_window_workspace,
    )
}

fn workspace_indicator_count_for_scope(
    compositor: CompositorType,
    current_slot: Option<usize>,
    workspaces: &[&Workspace],
    highest_window_workspace: usize,
) -> usize {
    let highest_reported = workspaces
        .iter()
        .filter_map(|workspace| match compositor {
            CompositorType::Niri => workspace.index,
            CompositorType::Hyprland | CompositorType::Unsupported => Some(workspace.id),
        })
        .max()
        .unwrap_or(0);

    highest_reported
        .max(current_slot.unwrap_or(0))
        .max(highest_window_workspace)
        .max(1)
}

fn current_workspace_slot(
    compositor: CompositorType,
    current_workspace: Option<usize>,
    workspaces: &[&Workspace],
) -> Option<usize> {
    match compositor {
        CompositorType::Niri => current_workspace.and_then(|id| {
            workspaces
                .iter()
                .find(|workspace| workspace.id == id)
                .and_then(|workspace| workspace.index)
        }),
        CompositorType::Hyprland | CompositorType::Unsupported => current_workspace,
    }
}

fn scoped_workspaces<'a>(
    compositor: CompositorType,
    panel_monitor: Option<&str>,
    current_workspace: Option<usize>,
    workspaces: &'a [Workspace],
) -> Vec<&'a Workspace> {
    if let Some(monitor) = panel_monitor {
        return workspaces
            .iter()
            .filter(|workspace| workspace.monitor.as_deref() == Some(monitor))
            .collect();
    }

    let all = || workspaces.iter().collect::<Vec<_>>();
    match compositor {
        CompositorType::Niri => {
            let Some(current_monitor) = current_workspace
                .and_then(|id| workspaces.iter().find(|workspace| workspace.id == id))
                .and_then(|workspace| workspace.monitor.as_deref())
            else {
                return all();
            };

            workspaces
                .iter()
                .filter(|workspace| workspace.monitor.as_deref() == Some(current_monitor))
                .collect()
        }
        CompositorType::Hyprland | CompositorType::Unsupported => all(),
    }
}

fn workspace_belongs_to_panel(
    compositor: CompositorType,
    panel_monitor: Option<&str>,
    workspace_id: usize,
    workspaces: &[Workspace],
) -> bool {
    let Some(monitor) = panel_monitor else {
        return true;
    };

    if matches!(compositor, CompositorType::Unsupported) {
        return true;
    }

    workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
        .and_then(|workspace| workspace.monitor.as_deref())
        .map(|workspace_monitor| workspace_monitor == monitor)
        .unwrap_or(false)
}

fn current_workspace_tooltip(state: &PagerState) -> String {
    let Some(current) = state.effective_current_workspace() else {
        return "Workspaces".into();
    };

    let workspace = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == current);
    if let Some(name) = workspace
        .and_then(|workspace| workspace.name.as_deref())
        .filter(|name| !name.is_empty())
    {
        return format!("Workspace {name}");
    }

    let scoped_workspaces = scoped_workspaces(
        state.compositor,
        state.panel_monitor.as_deref(),
        Some(current),
        &state.workspaces,
    );
    let label = current_workspace_slot(state.compositor, Some(current), &scoped_workspaces)
        .unwrap_or(current);
    format!("Workspace {label}")
}

fn current_workspace_window_tooltip(state: &PagerState) -> String {
    let workspace = current_workspace_tooltip(state);
    let windows = state
        .effective_current_workspace()
        .map(|current| {
            state
                .windows
                .iter()
                .filter(|window| window.workspace == Some(current))
                .count()
        })
        .unwrap_or(0);
    format!("{workspace}, {windows} windows")
}

fn occupied_workspaces(windows: &[PagerWindow]) -> HashSet<usize> {
    windows
        .iter()
        .filter_map(|window| window.workspace)
        .collect()
}

fn urgent_workspaces(windows: &[PagerWindow]) -> HashSet<usize> {
    windows
        .iter()
        .filter(|window| window.urgent)
        .filter_map(|window| window.workspace)
        .collect()
}

fn scroll_command(config: &Config, _state: &PagerState, next: bool, _horizontal: bool) -> Command {
    match config.display {
        DisplayMode::Windows => {
            if next {
                Command::FocusNextWindow
            } else {
                Command::FocusPreviousWindow
            }
        }
        DisplayMode::Workspaces => {
            if next {
                Command::FocusNextWorkspace
            } else {
                Command::FocusPreviousWorkspace
            }
        }
    }
}

fn command_for_target(target: PagerTarget) -> Option<Command> {
    match target {
        PagerTarget::Workspace(workspace) => Some(Command::SetWorkspace(workspace)),
        PagerTarget::Window(window) => Some(Command::FocusWindow(window)),
        PagerTarget::Placeholder => None,
    }
}

fn install_scroll_controller(root: &gtk::Box, sender: &ComponentSender<Applet>) {
    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL
            | gtk::EventControllerScrollFlags::HORIZONTAL
            | gtk::EventControllerScrollFlags::DISCRETE,
    );
    let scroll_sender = sender.clone();
    scroll.connect_scroll(move |_ctrl, dx, dy| {
        if let Some((next, horizontal)) = scroll_direction(dx, dy) {
            scroll_sender.input(Input::Scroll { next, horizontal });
        }

        gtk::glib::Propagation::Stop
    });
    root.add_controller(scroll);
}

fn scroll_direction(dx: f64, dy: f64) -> Option<(bool, bool)> {
    if dx == 0.0 && dy == 0.0 {
        return None;
    }

    if dx.abs() > dy.abs() {
        Some((dx > 0.0, true))
    } else {
        Some((dy > 0.0, false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::compositors::CompositorCapabilities;
    use toml::map::Map;

    #[test]
    fn default_config_matches_pager_defaults() {
        let config = Config::default();

        assert_eq!(config.display, DisplayMode::Windows);
        assert_eq!(config.appearance, PagerAppearance::Dots);
    }

    #[test]
    fn config_accepts_absent_and_empty_settings() {
        assert_eq!(Config::from_raw(&None), Config::default());
        assert_eq!(
            Config::from_raw(&Some(AppletConfig::default())),
            Config::default()
        );
    }

    #[test]
    fn config_parses_pager_settings() {
        let config = Config::from_raw(&Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(Map::from_iter([
                ("display".into(), toml::Value::String("workspaces".into())),
                ("appearance".into(), toml::Value::String("numbers".into())),
            ])),
        }));

        assert_eq!(config.display, DisplayMode::Workspaces);
        assert_eq!(config.appearance, PagerAppearance::Numbers);
    }

    #[test]
    fn config_ignores_legacy_style_setting() {
        let config = Config::from_raw(&Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(Map::from_iter([
                ("style".into(), toml::Value::String("numbered".into())),
                ("display".into(), toml::Value::String("workspaces".into())),
            ])),
        }));

        assert_eq!(config.display, DisplayMode::Workspaces);
    }

    #[test]
    fn config_rejects_unknown_settings_fields() {
        let config = Config::from_raw(&Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(Map::from_iter([(
                "unknown".into(),
                toml::Value::Boolean(true),
            )])),
        }));

        assert_eq!(config, Config::default());
    }

    #[test]
    fn indicator_count_comes_from_current_and_reported_workspaces() {
        assert_eq!(
            workspace_indicator_count(CompositorType::Hyprland, Some(11), &[], &[]),
            11
        );
        assert_eq!(
            workspace_indicator_count(CompositorType::Hyprland, None, &[workspace(12)], &[]),
            12
        );
        assert_eq!(
            workspace_indicator_count(
                CompositorType::Hyprland,
                None,
                &[],
                &[PagerWindow::from(&window(3, Some(3), false))]
            ),
            3
        );
        assert_eq!(
            workspace_indicator_count(CompositorType::Hyprland, None, &[], &[]),
            1
        );
    }

    #[test]
    fn workspace_items_mark_focused_occupied_urgent_and_named_workspaces() {
        let mut state = state_with_workspaces(vec![Workspace {
            id: 2,
            index: Some(2),
            name: Some("web".into()),
            monitor: None,
            active: true,
            focused: true,
            urgent: false,
            active_window: Some(20),
        }]);
        state.current_workspace = Some(2);
        state.windows = vec![
            window(20, Some(2), false),
            window(30, Some(3), true),
            window(40, None, true),
        ];
        let windows = state
            .windows
            .iter()
            .map(PagerWindow::from)
            .collect::<Vec<_>>();

        let items = workspace_items(
            state.compositor,
            state.current_workspace,
            None,
            true,
            &state.workspaces,
            &windows,
            PagerAppearance::Dots,
        );

        assert_eq!(items.len(), 3);
        assert!(items[1].focused);
        assert!(items[1].occupied);
        assert!(items[2].urgent);
        assert_eq!(
            items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["1", "2", "3"]
        );
    }

    #[test]
    fn view_uses_configured_pager_appearance() {
        let state = state_with_workspaces(vec![workspace(1), workspace(2), workspace(3)]);
        let config = Config {
            display: DisplayMode::Workspaces,
            appearance: PagerAppearance::Numbers,
            ..Config::default()
        };

        let view = view_from_state(&config, &PagerState::from(&state));

        assert_eq!(view.appearance, PagerAppearance::Numbers);
        assert_eq!(
            view.items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["1", "2", "3"]
        );
    }

    #[test]
    fn niri_view_renders_current_workspace_windows_as_focus_targets() {
        let mut state = state_with_workspaces(vec![
            Workspace {
                id: 7,
                index: Some(2),
                name: None,
                monitor: Some("eDP-1".into()),
                active: true,
                focused: true,
                urgent: false,
                active_window: Some(33),
            },
            Workspace {
                id: 8,
                index: Some(3),
                name: None,
                monitor: Some("eDP-1".into()),
                active: true,
                focused: false,
                urgent: false,
                active_window: Some(44),
            },
        ]);
        state.compositor = CompositorType::Niri;
        state.current_workspace = Some(7);
        state.focused_window = Some(33);
        state.capabilities.windows = true;
        state.capabilities.focused_window = true;
        state.windows = vec![
            window_with_position(11, Some(7), false, 20),
            window(44, Some(8), true),
            window_with_position(33, Some(7), false, 10),
        ];

        let view = view_from_state(&Config::default(), &PagerState::from(&state));

        assert_eq!(view.items.len(), 2);
        assert_eq!(view.items[0].id, 33);
        assert_eq!(view.items[0].target, PagerTarget::Window(33));
        assert_eq!(view.items[1].id, 11);
        assert_eq!(view.items[1].target, PagerTarget::Window(11));
        assert!(view.items[0].focused);
        assert!(view.items.iter().all(|item| item.occupied));
        assert!(!view.items.iter().any(|item| item.urgent));
        assert!(!view.placeholder);
    }

    #[test]
    fn niri_view_uses_placeholder_for_empty_current_workspace() {
        let mut state = state_with_workspaces(vec![Workspace {
            id: 7,
            index: Some(2),
            name: None,
            monitor: Some("eDP-1".into()),
            active: true,
            focused: true,
            urgent: false,
            active_window: None,
        }]);
        state.compositor = CompositorType::Niri;
        state.current_workspace = Some(7);
        state.capabilities.windows = true;
        state.capabilities.focused_window = true;
        state.windows = vec![window(44, Some(8), true)];

        let config = Config {
            appearance: PagerAppearance::Numbers,
            ..Config::default()
        };
        let view = view_from_state(&config, &PagerState::from(&state));

        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].target, PagerTarget::Placeholder);
        assert_eq!(view.items[0].appearance, PagerAppearance::Numbers);
        assert_eq!(view.items[0].label, "1");
        assert!(view.items[0].focused);
        assert!(!view.placeholder);
    }

    #[test]
    fn niri_view_uses_workspace_items_when_display_is_workspaces() {
        let mut state = state_with_workspaces(vec![
            Workspace {
                id: 7,
                index: Some(2),
                name: None,
                monitor: Some("eDP-1".into()),
                active: true,
                focused: true,
                urgent: false,
                active_window: Some(33),
            },
            Workspace {
                id: 8,
                index: Some(3),
                name: None,
                monitor: Some("eDP-1".into()),
                active: true,
                focused: false,
                urgent: false,
                active_window: Some(44),
            },
        ]);
        state.compositor = CompositorType::Niri;
        state.current_workspace = Some(7);
        state.capabilities.windows = true;
        state.capabilities.focused_window = true;
        state.windows = vec![window(33, Some(7), false), window(44, Some(8), false)];
        let config = Config {
            display: DisplayMode::Workspaces,
            ..Config::default()
        };

        let view = view_from_state(&config, &PagerState::from(&state));

        assert_eq!(view.items.len(), 3);
        assert_eq!(
            view.items
                .iter()
                .map(|item| item.target)
                .collect::<Vec<_>>(),
            vec![
                PagerTarget::Workspace(1),
                PagerTarget::Workspace(2),
                PagerTarget::Workspace(3)
            ]
        );
        assert!(view.items[1].focused);
        assert!(!view.placeholder);
    }

    #[test]
    fn niri_workspace_items_use_workspace_index_as_command_target() {
        let mut state = state_with_workspaces(vec![
            Workspace {
                id: 77,
                index: Some(2),
                name: Some("web".into()),
                monitor: Some("eDP-1".into()),
                active: true,
                focused: true,
                urgent: false,
                active_window: None,
            },
            Workspace {
                id: 88,
                index: Some(2),
                name: Some("chat".into()),
                monitor: Some("HDMI-A-1".into()),
                active: true,
                focused: false,
                urgent: false,
                active_window: None,
            },
        ]);
        state.compositor = CompositorType::Niri;
        state.current_workspace = Some(77);
        let windows = state
            .windows
            .iter()
            .map(PagerWindow::from)
            .collect::<Vec<_>>();

        let items = workspace_items(
            state.compositor,
            state.current_workspace,
            None,
            true,
            &state.workspaces,
            &windows,
            PagerAppearance::Dots,
        );

        assert_eq!(items[1].id, 2);
        assert!(items[1].focused);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn view_hides_when_compositor_has_no_workspace_support() {
        let mut state = State::default();
        state.capabilities.workspaces = false;

        let view = view_from_state(&Config::default(), &PagerState::from(&state));

        assert!(!view.visible);
        assert!(view.items.is_empty());
    }

    #[test]
    fn view_stays_visible_when_identity_arrives_before_structure_snapshot() {
        let mut state = State::default();
        state.compositor = CompositorType::Niri;
        state.capabilities.workspaces = true;
        state.capabilities.windows = true;
        state.capabilities.focused_window = true;

        let view = view_from_state(&Config::default(), &PagerState::from(&state));

        assert!(view.visible);
        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].target, PagerTarget::Placeholder);
        assert!(!view.placeholder);
    }

    #[test]
    fn pager_state_ignores_compositor_fields_unrelated_to_rendering() {
        let mut state = state_with_workspaces(vec![workspace(2)]);
        state.windows = vec![window(7, Some(2), false)];
        let mut unrelated = state.clone();
        unrelated.capabilities.monitors = true;
        unrelated.capabilities.night_light = true;
        unrelated.current_keyboard_layout = Some(1);
        unrelated.windows[0].title = Some("renamed".into());
        unrelated.windows[0].app_id = Some("app".into());
        unrelated.windows[0].pid = Some(42);
        unrelated.windows[0].fullscreen = true;
        unrelated.windows[0].floating = Some(true);

        assert_eq!(PagerState::from(&state), PagerState::from(&unrelated));
    }

    #[test]
    fn scroll_commands_honor_display() {
        let mut state = State::default();
        state.capabilities.windows = true;
        state.capabilities.focused_window = true;

        assert!(matches!(
            scroll_command(&Config::default(), &PagerState::from(&state), true, true),
            Command::FocusNextWindow
        ));
        assert!(matches!(
            scroll_command(&Config::default(), &PagerState::from(&state), true, false),
            Command::FocusNextWindow
        ));

        let config = Config {
            display: DisplayMode::Workspaces,
            ..Config::default()
        };
        assert!(matches!(
            scroll_command(&config, &PagerState::from(&state), false, false),
            Command::FocusPreviousWorkspace
        ));

        let config = Config {
            display: DisplayMode::Windows,
            ..Config::default()
        };
        assert!(matches!(
            scroll_command(&config, &PagerState::from(&State::default()), false, true),
            Command::FocusPreviousWindow
        ));
    }

    #[test]
    fn scroll_direction_uses_dominant_axis_and_ignores_zero_delta() {
        assert_eq!(scroll_direction(0.0, 0.0), None);
        assert_eq!(scroll_direction(0.2, 1.0), Some((true, false)));
        assert_eq!(scroll_direction(-1.0, 0.2), Some((false, true)));
    }

    fn state_with_workspaces(workspaces: Vec<Workspace>) -> State {
        State {
            capabilities: CompositorCapabilities {
                workspaces: true,
                current_workspace: true,
                ..CompositorCapabilities::default()
            },
            workspaces,
            ..State::default()
        }
    }

    fn workspace(id: usize) -> Workspace {
        Workspace {
            id,
            index: Some(id),
            name: None,
            monitor: None,
            active: false,
            focused: false,
            urgent: false,
            active_window: None,
        }
    }

    fn window(id: usize, workspace: Option<usize>, urgent: bool) -> Window {
        window_with_position(id, workspace, urgent, 0)
    }

    fn window_with_position(
        id: usize,
        workspace: Option<usize>,
        urgent: bool,
        layout_order: usize,
    ) -> Window {
        Window {
            id,
            title: None,
            app_id: None,
            pid: None,
            layout_order: Some(layout_order),
            workspace,
            focused: false,
            urgent,
            fullscreen: false,
            floating: None,
        }
    }

    fn niri_two_monitor_state() -> State {
        let mut state = state_with_workspaces(vec![
            Workspace {
                id: 1,
                index: Some(1),
                name: None,
                monitor: Some("eDP-1".into()),
                active: true,
                focused: false,
                urgent: false,
                active_window: Some(101),
            },
            Workspace {
                id: 2,
                index: Some(2),
                name: None,
                monitor: Some("eDP-1".into()),
                active: false,
                focused: false,
                urgent: false,
                active_window: None,
            },
            Workspace {
                id: 3,
                index: Some(1),
                name: None,
                monitor: Some("HDMI-A-1".into()),
                active: true,
                focused: true,
                urgent: false,
                active_window: Some(202),
            },
        ]);
        state.compositor = CompositorType::Niri;
        state.capabilities.windows = true;
        state.capabilities.focused_window = true;
        state.capabilities.monitors = true;
        state.current_workspace = Some(3);
        state.focused_window = Some(202);
        state.windows = vec![
            window(101, Some(1), false),
            window(202, Some(3), false),
        ];
        state.monitors = vec![
            glimpse_core::compositors::Monitor {
                id: Some(1),
                name: "eDP-1".into(),
                description: None,
                active_workspace: Some(1),
                focused: false,
            },
            glimpse_core::compositors::Monitor {
                id: Some(2),
                name: "HDMI-A-1".into(),
                description: None,
                active_workspace: Some(3),
                focused: true,
            },
        ];
        state
    }

    #[test]
    fn niri_workspaces_view_shows_only_panels_monitor_when_focus_is_elsewhere() {
        let state = niri_two_monitor_state();
        let mut pager_state = PagerState::from(&state);
        pager_state.panel_monitor = Some("eDP-1".into());
        let config = Config {
            display: DisplayMode::Workspaces,
            ..Config::default()
        };

        let view = view_from_state(&config, &pager_state);

        assert_eq!(view.items.len(), 2);
        assert_eq!(view.items[0].target, PagerTarget::Workspace(1));
        assert_eq!(view.items[1].target, PagerTarget::Workspace(2));
        assert!(view.items[0].focused);
        assert!(!view.items[1].focused);
    }

    #[test]
    fn niri_windows_view_lists_only_panels_monitor_active_workspace_windows() {
        let state = niri_two_monitor_state();
        let mut pager_state = PagerState::from(&state);
        pager_state.panel_monitor = Some("eDP-1".into());
        let config = Config {
            display: DisplayMode::Windows,
            ..Config::default()
        };

        let view = view_from_state(&config, &pager_state);

        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].target, PagerTarget::Window(101));
    }

    #[test]
    fn niri_windows_view_falls_back_to_placeholder_when_panel_monitor_has_no_active_workspace() {
        let mut state = niri_two_monitor_state();
        state.monitors[0].active_workspace = None;
        state.workspaces.retain(|workspace| workspace.id != 1);
        state.windows.retain(|window| window.workspace != Some(1));
        let mut pager_state = PagerState::from(&state);
        pager_state.panel_monitor = Some("eDP-1".into());
        let config = Config {
            display: DisplayMode::Windows,
            ..Config::default()
        };

        let view = view_from_state(&config, &pager_state);

        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].target, PagerTarget::Placeholder);
    }

    #[test]
    fn unset_panel_monitor_preserves_focused_monitor_scoping() {
        let state = niri_two_monitor_state();
        let pager_state = PagerState::from(&state);
        let config = Config {
            display: DisplayMode::Workspaces,
            ..Config::default()
        };

        let view = view_from_state(&config, &pager_state);

        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].target, PagerTarget::Workspace(1));
    }

    #[test]
    fn workspace_items_mark_active_slot_focused_on_focused_monitor() {
        let state = niri_two_monitor_state();
        let mut pager_state = PagerState::from(&state);
        pager_state.panel_monitor = Some("HDMI-A-1".into());
        let config = Config {
            display: DisplayMode::Workspaces,
            ..Config::default()
        };

        let view = view_from_state(&config, &pager_state);

        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].target, PagerTarget::Workspace(1));
        assert!(view.items[0].focused);
        assert!(view.items[0].monitor_focused);
    }

    #[test]
    fn workspace_items_mark_active_slot_inactive_when_panel_monitor_is_not_focused() {
        let state = niri_two_monitor_state();
        let mut pager_state = PagerState::from(&state);
        pager_state.panel_monitor = Some("eDP-1".into());
        let config = Config {
            display: DisplayMode::Workspaces,
            ..Config::default()
        };

        let view = view_from_state(&config, &pager_state);

        assert_eq!(view.items.len(), 2);
        assert!(view.items[0].focused);
        assert!(
            !view.items[0].monitor_focused,
            "monitor_focused must be false when global focus is on another output"
        );
        assert!(!view.items[1].focused);
    }

    #[test]
    fn hyprland_workspaces_view_filters_by_panel_monitor() {
        let mut state = state_with_workspaces(vec![
            Workspace {
                id: 1,
                index: Some(1),
                name: None,
                monitor: Some("DP-1".into()),
                active: true,
                focused: false,
                urgent: false,
                active_window: None,
            },
            Workspace {
                id: 2,
                index: Some(2),
                name: None,
                monitor: Some("DP-2".into()),
                active: true,
                focused: true,
                urgent: false,
                active_window: None,
            },
        ]);
        state.compositor = CompositorType::Hyprland;
        state.current_workspace = Some(2);
        state.monitors = vec![
            glimpse_core::compositors::Monitor {
                id: None,
                name: "DP-1".into(),
                description: None,
                active_workspace: Some(1),
                focused: false,
            },
            glimpse_core::compositors::Monitor {
                id: None,
                name: "DP-2".into(),
                description: None,
                active_workspace: Some(2),
                focused: true,
            },
        ];
        state.capabilities.monitors = true;

        let mut pager_state = PagerState::from(&state);
        pager_state.panel_monitor = Some("DP-1".into());
        let config = Config {
            display: DisplayMode::Workspaces,
            ..Config::default()
        };

        let view = view_from_state(&config, &pager_state);

        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].target, PagerTarget::Workspace(1));
        assert!(view.items[0].focused);
    }
}
