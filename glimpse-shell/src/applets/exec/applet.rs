#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, gio, prelude::*},
};
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::panels::applets::AppletConfig;

use super::{
    components::{StatusItem, StatusItemInit, StatusItemInput, StatusItemOutput},
    popover::{Input as PopoverInput, Output as PopoverOutput, Popover},
    protocol::{
        EventKind, EventPayload, EventSource, PanelCommand, PopoverPayload,
        StatusItem as StatusItemModel, StatusPayload, TreeNode,
    },
    supervisor::{self, Control},
};

const DEFAULT_RESTART_DELAY_MS: u64 = 1000;
const MIN_RESTART_DELAY_MS: u64 = 50;
const OUTBOUND_EVENT_BUFFER: usize = 128;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub command: Vec<String>,
    pub restart_delay_ms: u64,
    pub options: serde_json::Value,
    /// When true, drop the parent's environment before running the command.
    /// Defaults to false for backward compatibility; opt in to avoid leaking
    /// session-bus addresses, tokens, and other shell exports.
    pub env_clear: bool,
    /// Additional environment variables to set on the spawned process.
    /// Applied after env_clear, so they survive a cleared environment.
    pub env: std::collections::HashMap<String, String>,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        let mut config: Self = match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid exec applet config, using defaults");
                Self::default()
            }
        };
        config.normalize();
        config
    }

    fn normalize(&mut self) {
        if self.restart_delay_ms < MIN_RESTART_DELAY_MS {
            tracing::warn!(
                requested_ms = self.restart_delay_ms,
                clamped_ms = MIN_RESTART_DELAY_MS,
                "exec applet restart_delay_ms below minimum; clamping"
            );
            self.restart_delay_ms = MIN_RESTART_DELAY_MS;
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            command: Vec::new(),
            restart_delay_ms: DEFAULT_RESTART_DELAY_MS,
            options: serde_json::json!({}),
            env_clear: false,
            env: std::collections::HashMap::new(),
        }
    }
}

pub struct Applet {
    name: String,
    config: Config,
    status: Vec<StatusItemModel>,
    rendered_status: Vec<StatusItemModel>,
    root_node: Option<TreeNode>,
    rendered_has_popover: bool,
    popover_open: bool,
    root: gtk::Box,
    popover: Controller<Popover>,
    status_box: gtk::Box,
    status_items: Vec<RenderedStatusItem>,
    outbound_tx: mpsc::Sender<PanelCommand>,
    control_tx: mpsc::UnboundedSender<Control>,
    context_menu: gtk::PopoverMenu,
}

#[derive(Debug)]
pub struct Init {
    pub name: String,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    StatusChanged(StatusPayload),
    PopoverChanged(PopoverPayload),
    ChildExited,
    Reconfigure(Config),
    ShowContextMenu,
    RestartCommand,
    StatusItemOutput(StatusItemOutput),
    PopoverOutput(PopoverOutput),
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            set_orientation: gtk::Orientation::Horizontal,

            add_controller = gtk::GestureClick {
                set_button: 3,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(Input::ShowContextMenu);
                }
            },

            #[name = "status_box"]
            gtk::Box {
                add_css_class: "exec-status-box",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 0,
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = Popover::builder()
            .launch(super::popover::Init {
                parent: root.clone(),
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let (outbound_tx, outbound_rx) = mpsc::channel(OUTBOUND_EVENT_BUFFER);
        let (control_tx, control_rx) = mpsc::unbounded_channel();
        let name = init.name.clone();
        let config = init.config.clone();
        let supervisor_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            supervisor::run(name, config, outbound_rx, control_rx, supervisor_sender).await;
        });

        let context_menu = build_context_menu(&root, &sender);
        let widgets = view_output!();
        widgets.root.set_visible(false);

        let model = Applet {
            name: init.name,
            config: init.config,
            status: Vec::new(),
            rendered_status: Vec::new(),
            root_node: None,
            rendered_has_popover: false,
            popover_open: false,
            root: widgets.root.clone(),
            popover,
            status_box: widgets.status_box.clone(),
            status_items: Vec::new(),
            outbound_tx,
            control_tx,
            context_menu,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::StatusChanged(payload) => {
                self.status = payload.items;
                self.rebuild_status_if_needed(&sender);
            }
            Input::PopoverChanged(payload) => {
                let previous_has_popover = self.has_popover_content();
                self.root_node = payload.root;
                if previous_has_popover != self.has_popover_content() {
                    self.rebuild_status_if_needed(&sender);
                }
                if self.popover_open && self.has_popover_content() {
                    self.sync_popover();
                } else if self.popover_open {
                    self.popover.emit(PopoverInput::Close);
                }
            }
            Input::ChildExited => {
                self.status.clear();
                self.root_node = None;
                self.rendered_status.clear();
                self.rendered_has_popover = false;
                while let Some(child) = self.status_box.first_child() {
                    self.status_box.remove(&child);
                }
                self.status_items.clear();
                self.root.set_visible(false);
                self.popover.emit(PopoverInput::Close);
                self.context_menu.popdown();
            }
            Input::Reconfigure(config) => {
                if self.config == config {
                    return;
                }
                self.config = config.clone();
                self.popover.emit(PopoverInput::Close);
                self.context_menu.popdown();
                if let Err(error) = self.control_tx.send(Control::Reconfigure(config)) {
                    tracing::warn!(%error, applet = %self.name, "exec applet failed to reconfigure");
                }
            }
            Input::ShowContextMenu => {
                self.context_menu.popup();
            }
            Input::RestartCommand => {
                self.popover.emit(PopoverInput::Close);
                self.context_menu.popdown();
                if let Err(error) = self.control_tx.send(Control::Restart) {
                    tracing::warn!(%error, applet = %self.name, "exec applet failed to restart");
                }
            }
            Input::StatusItemOutput(output) => match output {
                StatusItemOutput::TogglePopover => {
                    if self.has_popover_content() {
                        self.popover.emit(PopoverInput::Toggle);
                    }
                }
                StatusItemOutput::ContextMenu => {
                    self.context_menu.popup();
                }
                StatusItemOutput::Event(event) => self.send_event(event),
                StatusItemOutput::Activate(event) => {
                    if let Some(event) = event {
                        self.send_event(event);
                    }
                    if self.has_popover_content() && !self.popover_open {
                        self.popover.emit(PopoverInput::Toggle);
                    }
                }
            },
            Input::PopoverOutput(PopoverOutput::Opened) => {
                if self.popover_open {
                    return;
                }
                self.popover_open = true;
                self.sync_popover();
                self.send_popover_lifecycle_event(EventKind::Open);
            }
            Input::PopoverOutput(PopoverOutput::Closed) => {
                if !self.popover_open {
                    return;
                }
                self.popover_open = false;
                self.send_popover_lifecycle_event(EventKind::Close);
            }
            Input::PopoverOutput(PopoverOutput::Event(event)) => self.send_event(event),
        }
    }
}

impl Applet {
    pub fn can_launch(config: &Config) -> bool {
        !config.command.is_empty()
    }

    fn has_popover_content(&self) -> bool {
        self.root_node.is_some()
    }

    fn sync_popover(&self) {
        if self.popover_open {
            self.popover
                .emit(PopoverInput::SetRoot(self.root_node.clone()));
        }
    }

    fn rebuild_status_if_needed(&mut self, sender: &ComponentSender<Self>) {
        let has_popover = self.has_popover_content();
        if self.rendered_status == self.status && self.rendered_has_popover == has_popover {
            return;
        }

        let mut existing = std::mem::take(&mut self.status_items);
        let mut next = Vec::with_capacity(self.status.len());
        let mut previous: Option<gtk::Widget> = None;
        for (index, item) in self.status.iter().enumerate() {
            let key = status_item_key(index, item);
            let controller =
                if let Some(position) = existing.iter().position(|rendered| rendered.key == key) {
                    let rendered = existing.remove(position);
                    rendered.controller.emit(StatusItemInput::Reconfigure {
                        item: item.clone(),
                        has_popover,
                    });
                    rendered.controller
                } else {
                    StatusItem::builder()
                        .launch(StatusItemInit {
                            item: item.clone(),
                            has_popover,
                        })
                        .forward(sender.input_sender(), Input::StatusItemOutput)
                };
            let widget = controller.widget().clone().upcast::<gtk::Widget>();
            place_status_widget(&self.status_box, &widget, previous.as_ref());
            previous = Some(widget);
            next.push(RenderedStatusItem { key, controller });
        }
        for rendered in existing {
            detach_status_widget(rendered.controller.widget());
        }
        self.status_items = next;

        self.rendered_status = self.status.clone();
        self.rendered_has_popover = has_popover;
        self.root.set_visible(!self.status.is_empty());
    }

    fn send_event(&self, event: EventPayload) {
        if let Err(error) = self.outbound_tx.try_send(PanelCommand::Event(event)) {
            tracing::warn!(%error, applet = %self.name, "exec applet failed to queue event");
        }
    }

    fn send_popover_lifecycle_event(&self, kind: EventKind) {
        self.send_event(EventPayload {
            id: "popover".into(),
            kind,
            source: EventSource::Popover,
            button: None,
            active: None,
            value: None,
            delta_y: None,
        });
    }
}

fn place_status_widget(container: &gtk::Box, widget: &gtk::Widget, sibling: Option<&gtk::Widget>) {
    match widget.parent() {
        Some(parent) if parent == container.clone().upcast::<gtk::Widget>() => {
            container.reorder_child_after(widget, sibling);
        }
        Some(_) => {
            detach_status_widget(widget);
            container.insert_child_after(widget, sibling);
        }
        None => {
            container.insert_child_after(widget, sibling);
        }
    }
}

fn detach_status_widget(widget: &impl IsA<gtk::Widget>) {
    if let Some(parent) = widget.as_ref().parent()
        && let Ok(parent) = parent.downcast::<gtk::Box>()
    {
        parent.remove(widget);
    }
}

struct RenderedStatusItem {
    key: String,
    controller: Controller<StatusItem>,
}

fn status_item_key(index: usize, item: &StatusItemModel) -> String {
    item.id
        .as_ref()
        .filter(|id| !id.is_empty())
        .map(|id| format!("id:{id}"))
        .unwrap_or_else(|| format!("index:{index}"))
}

fn build_context_menu(root: &gtk::Box, sender: &ComponentSender<Applet>) -> gtk::PopoverMenu {
    let action_group = gio::SimpleActionGroup::new();
    let restart_action = gio::SimpleAction::new("restart", None);
    restart_action.connect_activate({
        let sender = sender.input_sender().clone();
        move |_, _| {
            sender.emit(Input::RestartCommand);
        }
    });
    action_group.add_action(&restart_action);
    root.insert_action_group("exec", Some(&action_group));

    let menu = gio::Menu::new();
    menu.append(Some("Restart"), Some("exec.restart"));
    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(root);
    popover.set_has_arrow(false);
    root.connect_destroy({
        let popover = popover.clone();
        move |_| popover.unparent()
    });
    popover
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_command_configs_do_not_launch() {
        assert!(!Applet::can_launch(&Config::default()));
    }

    #[test]
    fn command_configs_can_launch() {
        let config = Config {
            command: vec!["/tmp/example".into()],
            ..Config::default()
        };

        assert!(Applet::can_launch(&config));
    }

    #[test]
    fn status_item_keys_prefer_protocol_ids() {
        let item = StatusItemModel {
            id: Some("cpu".into()),
            icon: None,
            label: Some("10%".into()),
            tooltip: None,
        };

        assert_eq!(status_item_key(3, &item), "id:cpu");
    }

    #[test]
    fn status_item_keys_fall_back_to_index_without_id() {
        let item = StatusItemModel {
            id: None,
            icon: None,
            label: Some("10%".into()),
            tooltip: None,
        };

        assert_eq!(status_item_key(3, &item), "index:3");
    }
}
