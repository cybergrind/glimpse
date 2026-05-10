#![allow(unused_assignments)]

use std::collections::HashSet;

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    compositors::{CompositorType, Window, Workspace},
    panels::applets::AppletConfig,
    services::{
        compositor::{Command, CompositorHandle, State},
        framework::ServiceCommand,
    },
};

use super::components::strip::{
    Input as StripInput, Kind, Output as StripOutput, PagerItem, PagerTarget, Strip, View,
};

const DEFAULT_COUNT: usize = 10;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScrollAction {
    Workspaces,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Original auto-detect: windows-of-focused-workspace on niri, workspaces elsewhere.
    Auto,
    /// Per-monitor workspaces with names (workspaces-pager).
    Workspaces,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    count: usize,
    scroll_action: Option<ScrollAction>,
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
            count: DEFAULT_COUNT,
            scroll_action: None,
        }
    }
}

pub struct Applet {
    config: Config,
    mode: Mode,
    monitor: Option<String>,
    state: PagerState,
    view: View,
    service: CompositorHandle,
    strip: Controller<Strip>,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: CompositorHandle,
    pub config: Config,
    pub mode: Mode,
    pub monitor: Option<String>,
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
            .launch(strip_kind(init.mode))
            .forward(sender.input_sender(), Input::StripOutput);
        let strip_widget = strip.widget().clone();
        let state = PagerState::from(&init.service.snapshot());
        let view = view_from_state(&init.config, init.mode, init.monitor.as_deref(), &state);

        let model = Applet {
            config: init.config,
            mode: init.mode,
            monitor: init.monitor,
            state,
            view,
            service: init.service,
            strip,
            subscription_cancel: CancellationToken::new(),
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
                let state = PagerState::from(&state);
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
                self.send_command(command_for_target(target));
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
        let view = view_from_state(&self.config, self.mode, self.monitor.as_deref(), &self.state);
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

fn strip_kind(mode: Mode) -> Kind {
    match mode {
        Mode::Auto => Kind::Classic,
        Mode::Workspaces => Kind::Workspaces,
    }
}

fn view_from_state(
    config: &Config,
    mode: Mode,
    panel_monitor: Option<&str>,
    state: &PagerState,
) -> View {
    match mode {
        Mode::Workspaces => desktops_view_from_state(config, panel_monitor, state),
        Mode::Auto => legacy_view_from_state(config, state),
    }
}

fn legacy_view_from_state(config: &Config, state: &PagerState) -> View {
    if !state.workspaces_available {
        return View {
            visible: false,
            tooltip: "Workspaces unavailable".into(),
            items: Vec::new(),
            placeholder: false,
        };
    }

    let (items, tooltip, placeholder) = match legacy_pager_mode(state) {
        LegacyPagerMode::Workspaces => (
            legacy_workspace_items(
                config.count,
                state.compositor,
                state.current_workspace,
                &state.workspaces,
                &state.windows,
            ),
            current_workspace_tooltip(None, state),
            false,
        ),
        LegacyPagerMode::Windows => {
            let items =
                legacy_window_items(state.current_workspace, state.focused_window, &state.windows);
            let count = items.len();
            let placeholder = items.is_empty();
            (
                items,
                current_workspace_window_tooltip(None, state, count),
                placeholder,
            )
        }
    };

    View {
        visible: true,
        tooltip,
        items,
        placeholder,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LegacyPagerMode {
    Workspaces,
    Windows,
}

fn legacy_pager_mode(state: &PagerState) -> LegacyPagerMode {
    if state.compositor == CompositorType::Niri && state.windows_available {
        LegacyPagerMode::Windows
    } else {
        LegacyPagerMode::Workspaces
    }
}

fn legacy_workspace_items(
    configured_count: usize,
    compositor: CompositorType,
    current_workspace: Option<usize>,
    workspaces: &[Workspace],
    windows: &[PagerWindow],
) -> Vec<PagerItem> {
    let occupied = occupied_workspaces(windows);
    let urgent = urgent_workspaces(windows);
    let scoped_workspaces = scoped_workspaces(compositor, None, current_workspace, workspaces);
    let current_slot =
        active_workspace_slot(compositor, None, current_workspace, &scoped_workspaces);
    let count = workspace_indicator_count_for_scope(
        configured_count,
        compositor,
        current_slot,
        &scoped_workspaces,
    );

    (1..=count)
        .map(|slot| {
            let workspace = workspace_for_slot(compositor, slot, &scoped_workspaces);
            let target = workspace_command_target(compositor, slot, workspace);
            let focused = workspace
                .map(|workspace| workspace.focused)
                .unwrap_or(current_slot == Some(slot));
            PagerItem {
                id: target,
                target: PagerTarget::Workspace(target),
                label: String::new(),
                active: false,
                focused,
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

fn legacy_window_items(
    current_workspace: Option<usize>,
    focused_window: Option<usize>,
    windows: &[PagerWindow],
) -> Vec<PagerItem> {
    let Some(current_workspace) = current_workspace else {
        return Vec::new();
    };

    let mut windows = windows
        .iter()
        .filter(|window| window.workspace == Some(current_workspace))
        .collect::<Vec<_>>();
    windows.sort_by_key(|window| (window.layout_order.unwrap_or(usize::MAX), window.id));

    windows
        .into_iter()
        .map(|window| PagerItem {
            id: window.id,
            target: PagerTarget::Window(window.id),
            label: String::new(),
            active: false,
            focused: window.focused || focused_window == Some(window.id),
            occupied: true,
            urgent: window.urgent,
        })
        .collect()
}

fn desktops_view_from_state(
    config: &Config,
    panel_monitor: Option<&str>,
    state: &PagerState,
) -> View {
    if !state.workspaces_available {
        return View {
            visible: false,
            tooltip: "Workspaces unavailable".into(),
            items: Vec::new(),
            placeholder: false,
        };
    }

    let items = workspace_items(
        config.count,
        state.compositor,
        panel_monitor,
        state.current_workspace,
        &state.workspaces,
        &state.windows,
    );
    let tooltip = current_workspace_tooltip(panel_monitor, state);

    View {
        visible: true,
        tooltip,
        items,
        placeholder: false,
    }
}

fn current_workspace_window_tooltip(
    panel_monitor: Option<&str>,
    state: &PagerState,
    window_count: usize,
) -> String {
    let workspace = current_workspace_tooltip(panel_monitor, state);
    format!("{workspace}, {window_count} windows")
}

fn settings_without_legacy_style(raw: &AppletConfig) -> toml::Value {
    let mut settings = raw.settings.clone();
    if let toml::Value::Table(table) = &mut settings {
        table.remove("style");
    }
    settings
}

fn workspace_items(
    configured_count: usize,
    compositor: CompositorType,
    panel_monitor: Option<&str>,
    current_workspace: Option<usize>,
    workspaces: &[Workspace],
    windows: &[PagerWindow],
) -> Vec<PagerItem> {
    let occupied = occupied_workspaces(windows);
    let urgent = urgent_workspaces(windows);
    let scoped_workspaces =
        scoped_workspaces(compositor, panel_monitor, current_workspace, workspaces);
    let current_slot =
        active_workspace_slot(compositor, panel_monitor, current_workspace, &scoped_workspaces);
    let count = workspace_indicator_count_for_scope(
        configured_count,
        compositor,
        current_slot,
        &scoped_workspaces,
    );

    (1..=count)
        .map(|slot| {
            let workspace = workspace_for_slot(compositor, slot, &scoped_workspaces);
            let target = workspace_command_target(compositor, slot, workspace);
            let active = match compositor {
                CompositorType::Niri => workspace.map(|workspace| workspace.active).unwrap_or(false),
                CompositorType::Hyprland | CompositorType::Unsupported => workspace
                    .map(|workspace| workspace.focused)
                    .unwrap_or(current_slot == Some(slot)),
            };
            let focused = workspace
                .map(|workspace| workspace.focused)
                .unwrap_or_else(|| panel_monitor.is_none() && current_slot == Some(slot));
            PagerItem {
                id: target,
                target: PagerTarget::Workspace(target),
                label: workspace_item_label(slot, workspace),
                active,
                focused,
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

fn workspace_item_label(slot: usize, workspace: Option<&Workspace>) -> String {
    workspace
        .and_then(|workspace| workspace.name.as_deref())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| slot.to_string())
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
    configured_count: usize,
    compositor: CompositorType,
    current_workspace: Option<usize>,
    workspaces: &[Workspace],
) -> usize {
    let scoped_workspaces = scoped_workspaces(compositor, None, current_workspace, workspaces);
    let current_slot =
        active_workspace_slot(compositor, None, current_workspace, &scoped_workspaces);
    workspace_indicator_count_for_scope(
        configured_count,
        compositor,
        current_slot,
        &scoped_workspaces,
    )
}

fn workspace_indicator_count_for_scope(
    configured_count: usize,
    compositor: CompositorType,
    current_slot: Option<usize>,
    workspaces: &[&Workspace],
) -> usize {
    let highest_reported = workspaces
        .iter()
        .filter_map(|workspace| match compositor {
            CompositorType::Niri => workspace.index,
            CompositorType::Hyprland | CompositorType::Unsupported => Some(workspace.id),
        })
        .max()
        .unwrap_or(0);

    match compositor {
        CompositorType::Niri => highest_reported.max(current_slot.unwrap_or(0)).max(1),
        CompositorType::Hyprland | CompositorType::Unsupported => highest_reported
            .max(current_slot.unwrap_or(0))
            .max(configured_count)
            .max(1),
    }
}

fn active_workspace_slot(
    compositor: CompositorType,
    panel_monitor: Option<&str>,
    current_workspace: Option<usize>,
    workspaces: &[&Workspace],
) -> Option<usize> {
    match compositor {
        CompositorType::Niri => {
            if panel_monitor.is_some() {
                workspaces
                    .iter()
                    .find(|workspace| workspace.active)
                    .and_then(|workspace| workspace.index)
            } else {
                current_workspace.and_then(|id| {
                    workspaces
                        .iter()
                        .find(|workspace| workspace.id == id)
                        .and_then(|workspace| workspace.index)
                })
            }
        }
        CompositorType::Hyprland | CompositorType::Unsupported => current_workspace,
    }
}

fn scoped_workspaces<'a>(
    compositor: CompositorType,
    panel_monitor: Option<&str>,
    current_workspace: Option<usize>,
    workspaces: &'a [Workspace],
) -> Vec<&'a Workspace> {
    let all = || workspaces.iter().collect::<Vec<_>>();
    match compositor {
        CompositorType::Niri => {
            let monitor = panel_monitor.or_else(|| {
                current_workspace
                    .and_then(|id| workspaces.iter().find(|workspace| workspace.id == id))
                    .and_then(|workspace| workspace.monitor.as_deref())
            });
            let Some(monitor) = monitor else {
                return all();
            };

            workspaces
                .iter()
                .filter(|workspace| workspace.monitor.as_deref() == Some(monitor))
                .collect()
        }
        CompositorType::Hyprland | CompositorType::Unsupported => all(),
    }
}

fn current_workspace_tooltip(panel_monitor: Option<&str>, state: &PagerState) -> String {
    let scoped_workspaces = scoped_workspaces(
        state.compositor,
        panel_monitor,
        state.current_workspace,
        &state.workspaces,
    );

    let active_workspace = if panel_monitor.is_some() {
        scoped_workspaces
            .iter()
            .copied()
            .find(|workspace| workspace.active)
            .or_else(|| {
                state.current_workspace.and_then(|id| {
                    scoped_workspaces
                        .iter()
                        .copied()
                        .find(|workspace| workspace.id == id)
                })
            })
    } else {
        state.current_workspace.and_then(|id| {
            state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == id)
        })
    };

    let Some(active) = active_workspace else {
        return "Workspaces".into();
    };

    if let Some(name) = active
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
    {
        return format!("Workspace {name}");
    }

    let label = active_workspace_slot(
        state.compositor,
        panel_monitor,
        state.current_workspace,
        &scoped_workspaces,
    )
    .unwrap_or(active.id);
    format!("Workspace {label}")
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

fn scroll_command(config: &Config, state: &PagerState, next: bool, horizontal: bool) -> Command {
    let action = config.scroll_action.unwrap_or_else(|| {
        if !horizontal && state.windows_available && state.focused_window_available {
            ScrollAction::Windows
        } else {
            ScrollAction::Workspaces
        }
    });

    match action {
        ScrollAction::Windows => {
            if next {
                Command::FocusNextWindow
            } else {
                Command::FocusPreviousWindow
            }
        }
        ScrollAction::Workspaces => {
            if next {
                Command::FocusNextWorkspace
            } else {
                Command::FocusPreviousWorkspace
            }
        }
    }
}

fn command_for_target(target: PagerTarget) -> Command {
    match target {
        PagerTarget::Workspace(workspace) => Command::SetWorkspace(workspace),
        PagerTarget::Window(window) => Command::FocusWindow(window),
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

        assert_eq!(config.count, DEFAULT_COUNT);
        assert_eq!(config.scroll_action, None);
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
                ("count".into(), toml::Value::Integer(4)),
                (
                    "scroll_action".into(),
                    toml::Value::String("workspaces".into()),
                ),
            ])),
        }));

        assert_eq!(config.count, 4);
        assert_eq!(config.scroll_action, Some(ScrollAction::Workspaces));
    }

    #[test]
    fn config_ignores_legacy_style_setting() {
        let config = Config::from_raw(&Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(Map::from_iter([
                ("style".into(), toml::Value::String("numbered".into())),
                ("count".into(), toml::Value::Integer(4)),
            ])),
        }));

        assert_eq!(config.count, 4);
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
    fn indicator_count_expands_to_include_current_and_reported_workspaces() {
        assert_eq!(
            workspace_indicator_count(10, CompositorType::Hyprland, Some(11), &[]),
            11
        );
        assert_eq!(
            workspace_indicator_count(10, CompositorType::Hyprland, None, &[workspace(12)]),
            12
        );
        assert_eq!(
            workspace_indicator_count(0, CompositorType::Hyprland, None, &[]),
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
            3,
            state.compositor,
            None,
            state.current_workspace,
            &state.workspaces,
            &windows,
        );

        assert_eq!(items.len(), 3);
        assert!(items[1].focused);
        assert!(items[1].occupied);
        assert!(items[2].urgent);
        assert_eq!(items[1].label, "web");
        assert_eq!(items[0].label, "1");
    }

    #[test]
    fn niri_view_scopes_workspaces_to_panel_monitor() {
        let mut state = state_with_workspaces(vec![
            Workspace {
                id: 7,
                index: Some(1),
                name: Some("code".into()),
                monitor: Some("eDP-1".into()),
                active: true,
                focused: true,
                urgent: false,
                active_window: Some(33),
            },
            Workspace {
                id: 8,
                index: Some(2),
                name: Some("web".into()),
                monitor: Some("eDP-1".into()),
                active: false,
                focused: false,
                urgent: false,
                active_window: None,
            },
            Workspace {
                id: 9,
                index: Some(1),
                name: Some("chat".into()),
                monitor: Some("HDMI-A-1".into()),
                active: true,
                focused: false,
                urgent: false,
                active_window: Some(44),
            },
        ]);
        state.compositor = CompositorType::Niri;
        state.current_workspace = Some(7);

        let view = view_from_state(
            &Config::default(),
            Mode::Workspaces,
            Some("HDMI-A-1"),
            &PagerState::from(&state),
        );

        assert_eq!(view.items.len(), 1);
        assert_eq!(view.items[0].label, "chat");
        assert!(view.items[0].active);
        assert!(!view.items[0].focused);
    }

    #[test]
    fn niri_view_marks_active_workspace_focused_for_unfocused_monitor() {
        let mut state = state_with_workspaces(vec![
            Workspace {
                id: 7,
                index: Some(1),
                name: None,
                monitor: Some("eDP-1".into()),
                active: true,
                focused: true,
                urgent: false,
                active_window: None,
            },
            Workspace {
                id: 9,
                index: Some(1),
                name: None,
                monitor: Some("HDMI-A-1".into()),
                active: true,
                focused: false,
                urgent: false,
                active_window: None,
            },
            Workspace {
                id: 10,
                index: Some(2),
                name: None,
                monitor: Some("HDMI-A-1".into()),
                active: false,
                focused: false,
                urgent: false,
                active_window: None,
            },
        ]);
        state.compositor = CompositorType::Niri;
        state.current_workspace = Some(7);

        let view = view_from_state(
            &Config::default(),
            Mode::Workspaces,
            Some("HDMI-A-1"),
            &PagerState::from(&state),
        );

        assert_eq!(view.items.len(), 2);
        assert!(view.items[0].active);
        assert!(!view.items[0].focused);
        assert!(!view.items[1].active);
        assert!(!view.items[1].focused);
    }

    #[test]
    fn niri_view_marks_focused_workspace_when_panel_on_focused_monitor() {
        let mut state = state_with_workspaces(vec![
            Workspace {
                id: 7,
                index: Some(1),
                name: None,
                monitor: Some("eDP-1".into()),
                active: true,
                focused: true,
                urgent: false,
                active_window: None,
            },
            Workspace {
                id: 9,
                index: Some(1),
                name: None,
                monitor: Some("HDMI-A-1".into()),
                active: true,
                focused: false,
                urgent: false,
                active_window: None,
            },
        ]);
        state.compositor = CompositorType::Niri;
        state.current_workspace = Some(7);

        let view_focused = view_from_state(
            &Config::default(),
            Mode::Workspaces,
            Some("eDP-1"),
            &PagerState::from(&state),
        );
        let view_other = view_from_state(
            &Config::default(),
            Mode::Workspaces,
            Some("HDMI-A-1"),
            &PagerState::from(&state),
        );

        assert!(view_focused.items[0].focused);
        assert!(view_focused.items[0].active);
        assert!(!view_other.items[0].focused);
        assert!(view_other.items[0].active);
    }

    #[test]
    fn niri_workspace_items_use_workspace_index_as_command_target() {
        let mut state = state_with_workspaces(vec![
            Workspace {
                id: 77,
                index: Some(1),
                name: Some("web".into()),
                monitor: Some("eDP-1".into()),
                active: true,
                focused: true,
                urgent: false,
                active_window: None,
            },
            Workspace {
                id: 88,
                index: Some(1),
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
            2,
            state.compositor,
            Some("eDP-1"),
            state.current_workspace,
            &state.workspaces,
            &windows,
        );

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, 1);
        assert_eq!(items[0].target, PagerTarget::Workspace(1));
        assert!(items[0].focused);
        assert!(items[0].active);
        assert_eq!(items[0].label, "web");
    }

    #[test]
    fn view_hides_when_compositor_has_no_workspace_support() {
        let mut state = State::default();
        state.capabilities.workspaces = false;

        let view = view_from_state(
            &Config::default(),
            Mode::Workspaces,
            None,
            &PagerState::from(&state),
        );

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

        let view = view_from_state(
            &Config::default(),
            Mode::Workspaces,
            None,
            &PagerState::from(&state),
        );

        assert!(view.visible);
    }

    #[test]
    fn pager_state_ignores_compositor_fields_unrelated_to_rendering() {
        let mut state = state_with_workspaces(vec![workspace(2)]);
        state.windows = vec![window(7, Some(2), false)];
        let mut unrelated = state.clone();
        unrelated.monitors = vec![glimpse_core::compositors::Monitor {
            id: Some(1),
            name: "eDP-1".into(),
            description: None,
            active_workspace: Some(2),
            focused: true,
        }];
        unrelated.capabilities.monitors = true;
        unrelated.capabilities.night_light = true;
        unrelated.windows[0].title = Some("renamed".into());
        unrelated.windows[0].app_id = Some("app".into());
        unrelated.windows[0].pid = Some(42);
        unrelated.windows[0].fullscreen = true;
        unrelated.windows[0].floating = Some(true);

        assert_eq!(PagerState::from(&state), PagerState::from(&unrelated));
    }

    #[test]
    fn auto_mode_on_niri_renders_windows_of_globally_focused_workspace() {
        let mut state = state_with_workspaces(vec![
            Workspace {
                id: 7,
                index: Some(1),
                name: None,
                monitor: Some("eDP-1".into()),
                active: true,
                focused: true,
                urgent: false,
                active_window: Some(33),
            },
            Workspace {
                id: 9,
                index: Some(1),
                name: None,
                monitor: Some("HDMI-A-1".into()),
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
            window(44, Some(9), false),
            window_with_position(33, Some(7), false, 10),
        ];

        let view = view_from_state(
            &Config::default(),
            Mode::Auto,
            Some("HDMI-A-1"),
            &PagerState::from(&state),
        );

        assert_eq!(view.items.len(), 2);
        assert_eq!(view.items[0].id, 33);
        assert_eq!(view.items[0].target, PagerTarget::Window(33));
        assert_eq!(view.items[1].id, 11);
        assert!(view.items[0].focused);
        assert!(view.items.iter().all(|item| item.occupied));
        assert!(!view.placeholder);
    }

    #[test]
    fn auto_mode_renders_placeholder_when_focused_workspace_has_no_windows() {
        let mut state = state_with_workspaces(vec![Workspace {
            id: 7,
            index: Some(1),
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

        let view = view_from_state(
            &Config::default(),
            Mode::Auto,
            None,
            &PagerState::from(&state),
        );

        assert!(view.items.is_empty());
        assert!(view.placeholder);
    }

    #[test]
    fn auto_mode_on_hyprland_renders_workspaces() {
        let mut state = state_with_workspaces(vec![Workspace {
            id: 1,
            index: None,
            name: None,
            monitor: None,
            active: false,
            focused: true,
            urgent: false,
            active_window: None,
        }]);
        state.compositor = CompositorType::Hyprland;
        state.current_workspace = Some(1);

        let view = view_from_state(
            &Config::default(),
            Mode::Auto,
            None,
            &PagerState::from(&state),
        );

        assert_eq!(view.items.len(), 10);
        assert_eq!(view.items[0].target, PagerTarget::Workspace(1));
        assert!(view.items[0].focused);
    }

    #[test]
    fn scroll_commands_honor_configured_axis_and_explicit_action() {
        let mut state = State::default();
        state.capabilities.windows = true;
        state.capabilities.focused_window = true;

        assert!(matches!(
            scroll_command(&Config::default(), &PagerState::from(&state), true, true),
            Command::FocusNextWorkspace
        ));
        assert!(matches!(
            scroll_command(&Config::default(), &PagerState::from(&state), true, false),
            Command::FocusNextWindow
        ));

        let config = Config {
            scroll_action: Some(ScrollAction::Workspaces),
            ..Config::default()
        };
        assert!(matches!(
            scroll_command(&config, &PagerState::from(&state), false, false),
            Command::FocusPreviousWorkspace
        ));

        let config = Config {
            scroll_action: Some(ScrollAction::Windows),
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
}
