use std::collections::HashMap;

use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, prelude::*},
};
use tokio::sync::mpsc;

use super::compositor::{self, AppletState, Compositor};
use super::config::{PagerConfig, PagerStyle, ScrollAction};

pub struct Pager {
    config: PagerConfig,
    compositor: Option<Compositor>,
    action_tx: mpsc::Sender<PagerAction>,
    indicators: HashMap<u32, gtk::Box>,
    container: gtk::Box,
    window_ids: HashMap<u32, u64>,
}

pub struct PagerInit {
    pub config: PagerConfig,
}

#[derive(Debug)]
pub enum PagerInput {
    Click(u32),
    Scroll { dy: f64, horizontal: bool },
}

#[derive(Debug)]
enum PagerAction {
    SwitchTo(u32),
    SwitchRelative(bool),
    FocusWindowRelative(bool),
    FocusWindow(u64),
}

#[relm4::component(pub)]
impl Component for Pager {
    type Init = PagerInit;
    type Input = PagerInput;
    type Output = ();
    type CommandOutput = AppletState;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "applet",
            add_css_class: "pager",
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let compositor = compositor::detect();
        let (action_tx, action_rx) = mpsc::channel::<PagerAction>(16);

        let model = Pager {
            config: init.config,
            compositor,
            action_tx,
            indicators: HashMap::new(),
            container: root.clone(),
            window_ids: HashMap::new(),
        };
        let widgets = view_output!();

        let scroll = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::HORIZONTAL,
        );
        let scroll_sender = sender.clone();
        scroll.connect_scroll(move |_ctrl, dx, dy| {
            if dx != 0.0 {
                scroll_sender.input(PagerInput::Scroll {
                    dy: dx,
                    horizontal: true,
                });
            } else if dy != 0.0 {
                scroll_sender.input(PagerInput::Scroll {
                    dy,
                    horizontal: false,
                });
            }
            gtk::glib::Propagation::Stop
        });
        root.add_controller(scroll);

        if let Some(comp) = compositor {
            sender.command(move |cmd_tx, shutdown| {
                shutdown
                    .register(async move {
                        let (state_tx, mut state_rx) = mpsc::channel::<AppletState>(16);
                        let mut action_rx = action_rx;

                        let event_handle = tokio::spawn(async move {
                            match comp {
                                Compositor::Hyprland => {
                                    compositor::hyprland_event_loop(state_tx).await;
                                }
                                Compositor::Niri => {
                                    compositor::niri_event_loop(state_tx).await;
                                }
                            }
                        });

                        loop {
                            tokio::select! {
                                Some(state) = state_rx.recv() => {
                                    if cmd_tx.send(state).is_err() {
                                        break;
                                    }
                                }
                                Some(action) = action_rx.recv() => {
                                    match action {
                                        PagerAction::SwitchTo(idx) => {
                                            compositor::switch_workspace(comp, idx).await;
                                        }
                                        PagerAction::SwitchRelative(next) => {
                                            compositor::switch_workspace_relative(comp, next).await;
                                        }
                                        PagerAction::FocusWindowRelative(next) => {
                                            compositor::focus_window_relative(comp, next).await;
                                        }
                                        PagerAction::FocusWindow(id) => {
                                            compositor::focus_window(id).await;
                                        }
                                    }
                                }
                                else => break,
                            }
                        }

                        event_handle.abort();
                    })
                    .drop_on_shutdown()
            });
        } else {
            tracing::warn!("pager: no supported compositor detected");
        }

        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        state: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match state {
            AppletState::Hyprland(ws_state) => {
                self.update_hyprland(ws_state, &sender);
                root.set_tooltip_text(None);
            }
            AppletState::Niri(win_state) => {
                let tooltip = format!(
                    "Workspace {}, {} windows",
                    win_state.workspace_index,
                    win_state.windows.len()
                );
                root.set_tooltip_text(Some(&tooltip));
                self.update_niri(win_state, &sender);
            }
        }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            PagerInput::Click(index) => match self.compositor {
                Some(Compositor::Niri) => {
                    if let Some(&window_id) = self.window_ids.get(&index) {
                        self.action_tx
                            .try_send(PagerAction::FocusWindow(window_id))
                            .ok();
                    }
                }
                Some(Compositor::Hyprland) => {
                    self.action_tx.try_send(PagerAction::SwitchTo(index)).ok();
                }
                None => {}
            },
            PagerInput::Scroll { dy, horizontal } => {
                let next = dy > 0.0;
                let scroll_action = self.resolve_scroll_action(horizontal);
                match scroll_action {
                    ResolvedScroll::SwitchWorkspace => {
                        self.action_tx
                            .try_send(PagerAction::SwitchRelative(next))
                            .ok();
                    }
                    ResolvedScroll::SwitchWindow => {
                        self.action_tx
                            .try_send(PagerAction::FocusWindowRelative(next))
                            .ok();
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

struct Indicator {
    index: u32,
    is_focused: bool,
    occupied: bool,
    is_urgent: bool,
}

impl Pager {
    fn resolve_scroll_action(&self, horizontal: bool) -> ResolvedScroll {
        match &self.config.scroll_action {
            Some(ScrollAction::Workspaces) => ResolvedScroll::SwitchWorkspace,
            Some(ScrollAction::Windows) => ResolvedScroll::SwitchWindow,
            None => {
                // Default: compositor-aware behavior
                match self.compositor {
                    Some(Compositor::Niri) => {
                        if horizontal {
                            ResolvedScroll::SwitchWorkspace
                        } else {
                            ResolvedScroll::SwitchWindow
                        }
                    }
                    _ => ResolvedScroll::SwitchWorkspace,
                }
            }
        }
    }

    fn update_hyprland(
        &mut self,
        state: compositor::WorkspaceState,
        sender: &ComponentSender<Self>,
    ) {
        let indicators: Vec<Indicator> = (1..=self.config.count)
            .map(|i| {
                let ws = state.workspaces.iter().find(|w| w.index == i);
                Indicator {
                    index: i,
                    is_focused: ws.map_or(false, |w| w.is_focused),
                    occupied: ws.map_or(false, |w| w.occupied),
                    is_urgent: ws.map_or(false, |w| w.is_urgent),
                }
            })
            .collect();
        self.update_indicators(&indicators, sender);
    }

    fn update_niri(&mut self, state: compositor::NiriWindowState, sender: &ComponentSender<Self>) {
        self.window_ids.clear();
        let indicators: Vec<Indicator> = if state.windows.is_empty() {
            vec![Indicator {
                index: 1,
                is_focused: true,
                occupied: false,
                is_urgent: false,
            }]
        } else {
            state
                .windows
                .iter()
                .enumerate()
                .map(|(i, w)| {
                    let index = (i + 1) as u32;
                    self.window_ids.insert(index, w.id);
                    Indicator {
                        index,
                        is_focused: w.is_focused,
                        occupied: true,
                        is_urgent: false,
                    }
                })
                .collect()
        };
        self.update_indicators(&indicators, sender);
    }

    fn update_indicators(&mut self, targets: &[Indicator], sender: &ComponentSender<Self>) {
        let current_indices: Vec<u32> = self.indicators.keys().copied().collect();
        let target_indices: Vec<u32> = targets.iter().map(|t| t.index).collect();

        for idx in &current_indices {
            if !target_indices.contains(idx) {
                if let Some(widget) = self.indicators.remove(idx) {
                    self.container.remove(&widget);
                }
            }
        }

        for t in targets {
            if !self.indicators.contains_key(&t.index) {
                let widget = self.create_indicator(t.index, sender);
                self.container.append(&widget);
                self.indicators.insert(t.index, widget);
            }
            let indicator = self.indicators.get(&t.index).unwrap();

            Self::set_class(indicator, "active", t.is_focused);
            Self::set_class(indicator, "occupied", t.occupied && !t.is_focused);
            Self::set_class(indicator, "urgent", t.is_urgent);

            if self.config.style == PagerStyle::Numbered {
                if let Some(label) = indicator
                    .first_child()
                    .and_then(|c| c.downcast::<gtk::Label>().ok())
                {
                    label.set_label(&t.index.to_string());
                }
            }
        }

        let mut prev: Option<&gtk::Box> = None;
        for idx in &target_indices {
            if let Some(widget) = self.indicators.get(idx) {
                if let Some(p) = prev {
                    widget.insert_after(&self.container, Some(p));
                } else {
                    widget.insert_after(&self.container, gtk::Widget::NONE);
                }
                prev = Some(widget);
            }
        }
    }

    fn create_indicator(&self, index: u32, sender: &ComponentSender<Self>) -> gtk::Box {
        let wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        wrapper.set_valign(gtk::Align::Center);

        match self.config.style {
            PagerStyle::Pills => {
                wrapper.add_css_class("pager-dot");
            }
            PagerStyle::Numbered => {
                wrapper.add_css_class("pager-num");
                let label = gtk::Label::new(Some(&index.to_string()));
                wrapper.append(&label);
            }
        }

        let click = gtk::GestureClick::new();
        click.set_button(1);
        let click_sender = sender.clone();
        click.connect_pressed(move |_, _, _, _| {
            click_sender.input(PagerInput::Click(index));
        });
        wrapper.add_controller(click);

        wrapper
    }

    fn set_class(widget: &gtk::Box, class: &str, active: bool) {
        if active {
            widget.add_css_class(class);
        } else {
            widget.remove_css_class(class);
        }
    }
}
