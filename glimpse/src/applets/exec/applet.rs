use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use tokio::sync::mpsc;

use super::{
    components::{context_menu::build_context_menu, status_bar::build_status_item},
    popover::{ExecPopover, ExecPopoverInit, ExecPopoverInput, ExecPopoverOutput},
    protocol::{CallbackData, ChildMessage, PanelMessage, StatusItem, TreeNode},
    supervisor::{SupervisorControl, run_supervisor},
    ExecConfig,
};

pub struct Exec {
    pub(super) name: String,
    config: ExecConfig,
    status: Vec<StatusItem>,
    tree: Option<TreeNode>,
    outbound_tx: mpsc::UnboundedSender<PanelMessage>,
    restart_tx: mpsc::UnboundedSender<SupervisorControl>,
    popover: Controller<ExecPopover>,
    trigger: gtk::MenuButton,
    status_box: gtk::Box,
    context_menu: gtk::PopoverMenu,
}

#[derive(Clone)]
pub struct ExecInit {
    pub name: String,
    pub config: ExecConfig,
}

#[derive(Debug)]
pub enum ExecMsg {
    ChildMessage(ChildMessage),
    ChildExited,
    Reconfigure(ExecConfig),
    Callback(CallbackData),
    RestartCommand,
    TogglePopover,
}

#[relm4::component(pub)]
impl Component for Exec {
    type Init = ExecInit;
    type Input = ExecMsg;
    type Output = ();
    type CommandOutput = ExecMsg;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "exec",
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = ExecPopover::builder()
            .launch(ExecPopoverInit {
                applet_name: init.name.clone(),
            })
            .forward(sender.input_sender(), |msg| match msg {
                ExecPopoverOutput::Callback(cb) => ExecMsg::Callback(cb),
            });

        let trigger = gtk::MenuButton::new();
        trigger.set_has_frame(false);
        trigger.add_css_class("flat");
        trigger.add_css_class("exec-trigger");
        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        status_box.add_css_class("exec-status-box");
        trigger.set_child(Some(&status_box));
        trigger.set_popover(Some(popover.widget()));
        root.append(&trigger);
        root.set_visible(false);

        let context_menu = build_context_menu(&root, &sender);

        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (restart_tx, restart_rx) = mpsc::unbounded_channel();
        let name = init.name.clone();
        let config = init.config.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    run_supervisor(name, config, outbound_rx, restart_rx, out).await;
                })
                .drop_on_shutdown()
        });

        let model = Exec {
            name: init.name,
            config: init.config,
            status: Vec::new(),
            tree: None,
            outbound_tx,
            restart_tx,
            popover,
            trigger,
            status_box,
            context_menu,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            ExecMsg::ChildMessage(message) => {
                match message {
                    ChildMessage::Status(data) => self.status = data.items,
                    ChildMessage::Tree { content } => {
                        self.tree = content.clone();
                        self.popover.emit(ExecPopoverInput::SetTree(content));
                    }
                }
                self.rebuild_status(&sender);
                root.set_visible(!self.status.is_empty());
            }
            ExecMsg::ChildExited => {
                self.status.clear();
                self.tree = None;
                self.popover.emit(ExecPopoverInput::Clear);
                self.trigger.popdown();
                self.context_menu.popdown();
                self.rebuild_status(&sender);
                root.set_visible(false);
            }
            ExecMsg::Reconfigure(config) => {
                if self.config != config {
                    self.config = config.clone();
                    self.trigger.popdown();
                    self.context_menu.popdown();
                    if let Err(error) = self.restart_tx.send(SupervisorControl::Reconfigure(config)) {
                        tracing::warn!(%error, applet = %self.name, "exec applet: failed to reconfigure");
                    }
                }
            }
            ExecMsg::Callback(callback) => {
                self.popover
                    .emit(ExecPopoverInput::RememberInteraction(callback.id.clone()));
                if let Err(error) = self.outbound_tx.send(PanelMessage::Callback(callback)) {
                    tracing::warn!(%error, applet = %self.name, "exec applet: failed to queue callback");
                }
            }
            ExecMsg::RestartCommand => {
                self.trigger.popdown();
                self.context_menu.popdown();
                if let Err(error) = self.restart_tx.send(SupervisorControl::Restart) {
                    tracing::warn!(%error, applet = %self.name, "exec applet: failed to request restart");
                }
            }
            ExecMsg::TogglePopover => {
                if self.has_popover_content() {
                    if self.popover.widget().is_visible() {
                        self.trigger.popdown();
                    } else {
                        self.context_menu.popdown();
                        self.trigger.popup();
                    }
                }
            }
        }
    }
}

impl Exec {
    fn has_popover_content(&self) -> bool {
        self.tree.is_some()
    }

    fn rebuild_status(&self, sender: &ComponentSender<Self>) {
        while let Some(child) = self.status_box.first_child() {
            self.status_box.remove(&child);
        }
        for (index, item) in self.status.iter().enumerate() {
            self.status_box.append(&build_status_item(
                item,
                index,
                self.has_popover_content(),
                sender,
            ));
        }
    }
}
