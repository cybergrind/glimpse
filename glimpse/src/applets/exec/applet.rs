use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use tokio::sync::mpsc;

use super::{
    ExecConfig,
    components::{context_menu::build_context_menu, status_bar::build_status_item},
    popover::{ExecPopover, ExecPopoverInit, ExecPopoverInput, ExecPopoverOutput},
    protocol::{CallbackData, ChildMessage, HeroNode, PanelMessage, StatusItem, TreeNode},
    supervisor::{SupervisorControl, run_supervisor},
};

pub struct Exec {
    pub(super) name: String,
    config: ExecConfig,
    status: Vec<StatusItem>,
    hero: Option<HeroNode>,
    tree: Option<TreeNode>,
    outbound_tx: mpsc::UnboundedSender<PanelMessage>,
    restart_tx: mpsc::UnboundedSender<SupervisorControl>,
    popover: Controller<ExecPopover>,
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

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(ExecMsg::TogglePopover);
                }
            },
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
                parent: root.clone().upcast(),
            })
            .forward(sender.input_sender(), |msg| match msg {
                ExecPopoverOutput::Callback(cb) => ExecMsg::Callback(cb),
            });

        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        status_box.add_css_class("exec-status-box");
        root.append(&status_box);
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
            hero: None,
            tree: None,
            outbound_tx,
            restart_tx,
            popover,
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
                    ChildMessage::Hero(hero) => {
                        self.hero = Some(hero);
                        self.popover
                            .emit(ExecPopoverInput::SetTree(self.popover_tree()));
                    }
                    ChildMessage::Tree { content } => {
                        self.tree = content;
                        self.popover
                            .emit(ExecPopoverInput::SetTree(self.popover_tree()));
                    }
                }
                self.rebuild_status(&sender);
                root.set_visible(!self.display_status_items().is_empty());
            }
            ExecMsg::ChildExited => {
                self.status.clear();
                self.hero = None;
                self.tree = None;
                self.popover.emit(ExecPopoverInput::Clear);
                self.popover.widget().popdown();
                self.context_menu.popdown();
                self.rebuild_status(&sender);
                root.set_visible(false);
            }
            ExecMsg::Reconfigure(config) => {
                if self.config != config {
                    self.config = config.clone();
                    self.popover.widget().popdown();
                    self.context_menu.popdown();
                    if let Err(error) = self.restart_tx.send(SupervisorControl::Reconfigure(config))
                    {
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
                self.popover.widget().popdown();
                self.context_menu.popdown();
                if let Err(error) = self.restart_tx.send(SupervisorControl::Restart) {
                    tracing::warn!(%error, applet = %self.name, "exec applet: failed to request restart");
                }
            }
            ExecMsg::TogglePopover => {
                if self.has_popover_content() {
                    if self.popover.widget().is_visible() {
                        self.popover.widget().popdown();
                    } else {
                        self.context_menu.popdown();
                        self.popover.widget().popup();
                    }
                }
            }
        }
    }
}

impl Exec {
    fn has_popover_content(&self) -> bool {
        self.tree.is_some() || self.hero.is_some()
    }

    fn popover_tree(&self) -> Option<TreeNode> {
        self.tree
            .clone()
            .or_else(|| self.hero.clone().map(TreeNode::Hero))
    }

    fn display_status_items(&self) -> Vec<StatusItem> {
        Self::display_status_from_parts(&self.status, self.hero.as_ref())
    }

    fn display_status_from_parts(
        status: &[StatusItem],
        hero: Option<&HeroNode>,
    ) -> Vec<StatusItem> {
        if !status.is_empty() {
            return status.to_vec();
        }

        let Some(hero) = hero else {
            return Vec::new();
        };

        let text = if hero.subtitle.is_empty() {
            hero.title.clone()
        } else {
            hero.subtitle.clone()
        };

        vec![StatusItem {
            id: hero.common.id.clone(),
            icon: hero.icon.clone(),
            text: Some(text),
        }]
    }

    fn rebuild_status(&self, sender: &ComponentSender<Self>) {
        while let Some(child) = self.status_box.first_child() {
            self.status_box.remove(&child);
        }
        for (index, item) in self.display_status_items().iter().enumerate() {
            self.status_box.append(&build_status_item(
                item,
                index,
                self.has_popover_content(),
                sender,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Exec;
    use crate::applets::exec::protocol::{CommonProps, HeroNode, IconSource, StatusItem};

    #[test]
    fn hero_falls_back_to_panel_status_when_status_items_are_missing() {
        let status = Exec::display_status_from_parts(
            &[],
            Some(&HeroNode {
                common: CommonProps::default(),
                title: "System Stats".into(),
                subtitle: "CPU 6% · RAM 84%".into(),
                icon: Some(IconSource::Name("computer-symbolic".into())),
            }),
        );

        assert_eq!(
            status,
            vec![StatusItem {
                id: None,
                icon: Some(IconSource::Name("computer-symbolic".into())),
                text: Some("CPU 6% · RAM 84%".into()),
            }]
        );
    }

    #[test]
    fn explicit_status_items_take_priority_over_hero_fallback() {
        let status = Exec::display_status_from_parts(
            &[StatusItem {
                id: Some("cpu".into()),
                icon: None,
                text: Some("CPU 6%".into()),
            }],
            Some(&HeroNode {
                common: CommonProps::default(),
                title: "System Stats".into(),
                subtitle: "CPU 6% · RAM 84%".into(),
                icon: Some(IconSource::Name("computer-symbolic".into())),
            }),
        );

        assert_eq!(
            status,
            vec![StatusItem {
                id: Some("cpu".into()),
                icon: None,
                text: Some("CPU 6%".into()),
            }]
        );
    }
}
