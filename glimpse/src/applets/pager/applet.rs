#![allow(unused_assignments)]

use std::collections::HashMap;

use glimpse::compositor::{
    WorkspaceCommand, WorkspacePresentation, WorkspaceServiceHandle, WorkspaceState,
};
use relm4::{gtk, Component, ComponentController, ComponentParts, ComponentSender, Controller};

use super::components::indicator_strip::{
    PagerIndicatorStrip, PagerIndicatorStripInit, PagerIndicatorStripInput,
    PagerIndicatorStripOutput, PagerIndicatorView, PagerStripView,
};
use super::{PagerConfig, ScrollAction};

pub struct Pager {
    config: PagerConfig,
    service: WorkspaceServiceHandle,
    state: WorkspaceState,
    strip: Controller<PagerIndicatorStrip>,
    window_ids: HashMap<u32, u64>,
    last_focused_window_id: Option<u64>,
}

pub struct PagerInit {
    pub config: PagerConfig,
    pub service: WorkspaceServiceHandle,
}

#[derive(Debug, Clone)]
pub enum PagerInput {
    ServiceState(WorkspaceState),
    Reconfigure(PagerConfig),
    StripOutput(PagerIndicatorStripOutput),
}

#[relm4::component(pub)]
impl Component for Pager {
    type Init = PagerInit;
    type Input = PagerInput;
    type Output = ();
    type CommandOutput = PagerInput;

    view! {
        gtk::Box {
            #[local_ref]
            strip_widget -> gtk::Box {},
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let strip = PagerIndicatorStrip::builder()
            .launch(PagerIndicatorStripInit {
                style: init.config.style.clone(),
            })
            .forward(sender.input_sender(), PagerInput::StripOutput);
        let strip_widget = strip.widget().clone();

        let model = Pager {
            config: init.config,
            state: init.service.subscribe().borrow().clone(),
            service: init.service.clone(),
            strip,
            window_ids: HashMap::new(),
            last_focused_window_id: None,
        };
        let service = init.service;

        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    let mut state_rx = service.subscribe();
                    let _ = out.send(PagerInput::ServiceState(state_rx.borrow().clone()));

                    while state_rx.changed().await.is_ok() {
                        let _ = out.send(PagerInput::ServiceState(state_rx.borrow().clone()));
                    }
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(message, sender, root);
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            PagerInput::ServiceState(state) => {
                self.last_focused_window_id = remember_focused_window_id(
                    self.last_focused_window_id,
                    &state.snapshot.windows,
                );
                self.state = state;
                self.render_from_state();
            }
            PagerInput::Reconfigure(config) => {
                self.config = config;
                self.render_from_state();
            }
            PagerInput::StripOutput(PagerIndicatorStripOutput::Click(index)) => {
                match self.state.snapshot.presentation {
                    WorkspacePresentation::Windows => {
                        if let Some(&window_id) = self.window_ids.get(&index) {
                            self.send_command(sender, WorkspaceCommand::FocusWindow(window_id));
                        }
                    }
                    WorkspacePresentation::Workspaces => {
                        self.send_command(sender, WorkspaceCommand::SwitchTo(index));
                    }
                }
            }
            PagerInput::StripOutput(PagerIndicatorStripOutput::Scroll { dy, horizontal }) => {
                match self.resolve_scroll_action(horizontal) {
                    ResolvedScroll::SwitchWorkspace => {
                        self.send_command(sender, WorkspaceCommand::SwitchRelative(dy > 0.0));
                    }
                    ResolvedScroll::SwitchWindow => {
                        self.send_command(sender, WorkspaceCommand::FocusWindowRelative(dy > 0.0));
                    }
                }
            }
        }
    }
}

enum ResolvedScroll {
    SwitchWorkspace,
    SwitchWindow,
}

impl Pager {
    fn resolve_scroll_action(&self, horizontal: bool) -> ResolvedScroll {
        match &self.config.scroll_action {
            Some(ScrollAction::Workspaces) => ResolvedScroll::SwitchWorkspace,
            Some(ScrollAction::Windows) => ResolvedScroll::SwitchWindow,
            None => {
                if self.state.capabilities.focus_window_relative && !horizontal {
                    ResolvedScroll::SwitchWindow
                } else {
                    ResolvedScroll::SwitchWorkspace
                }
            }
        }
    }

    fn render_from_state(&mut self) {
        self.window_ids.clear();

        let (indicators, tooltip) = match self.state.snapshot.presentation {
            WorkspacePresentation::Workspaces => {
                let count = workspace_indicator_count(
                    self.config.count,
                    self.state.snapshot.current_workspace_index,
                    &self.state.snapshot.workspaces,
                );
                let indicators = (1..=count)
                    .map(|index| {
                        let ws = self
                            .state
                            .snapshot
                            .workspaces
                            .iter()
                            .find(|workspace| workspace.index == index);
                        PagerIndicatorView {
                            index,
                            is_focused: ws.is_some_and(|workspace| workspace.is_focused),
                            occupied: ws.is_some_and(|workspace| workspace.occupied),
                            is_urgent: ws.is_some_and(|workspace| workspace.is_urgent),
                        }
                    })
                    .collect();
                (indicators, String::new())
            }
            WorkspacePresentation::Windows => {
                let focused_window_id = render_focused_window_id(
                    &self.state.snapshot.windows,
                    self.last_focused_window_id,
                );
                let indicators = if self.state.snapshot.windows.is_empty() {
                    vec![PagerIndicatorView {
                        index: 1,
                        is_focused: true,
                        occupied: false,
                        is_urgent: false,
                    }]
                } else {
                    self.state
                        .snapshot
                        .windows
                        .iter()
                        .enumerate()
                        .map(|(offset, window)| {
                            let index = (offset + 1) as u32;
                            self.window_ids.insert(index, window.id);
                            PagerIndicatorView {
                                index,
                                is_focused: Some(window.id) == focused_window_id,
                                occupied: true,
                                is_urgent: false,
                            }
                        })
                        .collect()
                };
                let tooltip = format!(
                    "Workspace {}, {} windows",
                    self.state.snapshot.current_workspace_index.unwrap_or(0),
                    self.state.snapshot.windows.len()
                );
                (indicators, tooltip)
            }
        };

        self.strip
            .emit(PagerIndicatorStripInput::Render(PagerStripView {
                indicators,
                tooltip,
            }));
    }

    fn send_command(&self, sender: ComponentSender<Self>, command: WorkspaceCommand) {
        let service = self.service.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    if let Err(error) = service.send(command).await {
                        tracing::warn!(error = %error, "pager applet: failed to send workspace command");
                    }
                })
                .drop_on_shutdown()
        });
    }
}

fn workspace_indicator_count(
    configured_count: u32,
    current_workspace_index: Option<u32>,
    workspaces: &[glimpse::compositor::WorkspaceSlot],
) -> u32 {
    let highest_reported = workspaces
        .iter()
        .map(|workspace| workspace.index)
        .max()
        .unwrap_or(0);

    configured_count
        .max(current_workspace_index.unwrap_or(0))
        .max(highest_reported)
        .max(1)
}

fn remember_focused_window_id(current: Option<u64>, windows: &[glimpse::compositor::WorkspaceWindow]) -> Option<u64> {
    windows.iter().find(|window| window.is_focused).map(|window| window.id).or(current)
}

fn render_focused_window_id(
    windows: &[glimpse::compositor::WorkspaceWindow],
    last_focused_window_id: Option<u64>,
) -> Option<u64> {
    windows
        .iter()
        .find(|window| window.is_focused)
        .map(|window| window.id)
        .or_else(|| {
            last_focused_window_id.filter(|window_id| windows.iter().any(|window| window.id == *window_id))
        })
        .or_else(|| windows.first().map(|window| window.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indicator_count_expands_to_include_focused_or_reported_workspace() {
        let workspaces = vec![glimpse::compositor::WorkspaceSlot {
            index: 11,
            is_focused: true,
            occupied: true,
            is_urgent: false,
        }];

        assert_eq!(workspace_indicator_count(10, Some(11), &workspaces), 11);
        assert_eq!(workspace_indicator_count(10, None, &workspaces), 11);
        assert_eq!(workspace_indicator_count(10, Some(3), &[]), 10);
        assert_eq!(workspace_indicator_count(0, None, &[]), 1);
    }

    #[test]
    fn window_presentation_keeps_last_focused_window_when_focus_is_lost() {
        let first = glimpse::compositor::WorkspaceWindow {
            id: 10,
            column: 1,
            is_focused: true,
        };
        let second = glimpse::compositor::WorkspaceWindow {
            id: 20,
            column: 2,
            is_focused: false,
        };

        let remembered = remember_focused_window_id(None, &[first.clone(), second.clone()]);
        assert_eq!(remembered, Some(10));

        let unfocused = vec![
            glimpse::compositor::WorkspaceWindow {
                is_focused: false,
                ..first
            },
            second,
        ];
        assert_eq!(render_focused_window_id(&unfocused, remembered), Some(10));
    }

    #[test]
    fn window_presentation_falls_back_to_first_window_when_remembered_window_is_gone() {
        let windows = vec![
            glimpse::compositor::WorkspaceWindow {
                id: 20,
                column: 1,
                is_focused: false,
            },
            glimpse::compositor::WorkspaceWindow {
                id: 30,
                column: 2,
                is_focused: false,
            },
        ];

        assert_eq!(render_focused_window_id(&windows, Some(10)), Some(20));
    }
}
