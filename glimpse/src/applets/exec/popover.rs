use std::{collections::HashMap, rc::Rc};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use super::{
    protocol::{CallbackData, TreeNode},
    renderer::RenderCatalog,
};

pub struct ExecPopover {
    popover: gtk::Popover,
    tree: Option<TreeNode>,
    applet_name: String,
    last_interacted_id: Option<String>,
    focus_targets: HashMap<String, gtk::Widget>,
}

pub struct ExecPopoverInit {
    pub applet_name: String,
    pub parent: gtk::Widget,
}

#[derive(Debug)]
pub enum ExecPopoverInput {
    Clear,
    RememberInteraction(String),
    SetTree(Option<TreeNode>),
}

#[derive(Debug, Clone)]
pub enum ExecPopoverOutput {
    Callback(CallbackData),
}

impl SimpleComponent for ExecPopover {
    type Init = ExecPopoverInit;
    type Input = ExecPopoverInput;
    type Output = ExecPopoverOutput;
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Popover::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_autohide(true);
        root.add_css_class("exec-popover");
        root.set_parent(&init.parent);

        let model = ExecPopover {
            popover: root.clone(),
            tree: None,
            applet_name: init.applet_name,
            last_interacted_id: None,
            focus_targets: HashMap::new(),
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            ExecPopoverInput::Clear => {
                self.tree = None;
                self.last_interacted_id = None;
                self.focus_targets.clear();
                self.popover.set_child(Option::<&gtk::Widget>::None);
                self.popover.popdown();
            }
            ExecPopoverInput::RememberInteraction(id) => {
                self.last_interacted_id = Some(id);
            }
            ExecPopoverInput::SetTree(tree) => {
                self.tree = tree;
                self.rebuild(&sender);
            }
        }
    }
}

impl ExecPopover {
    fn rebuild(&mut self, sender: &ComponentSender<Self>) {
        self.remember_current_focus();
        let Some(tree) = &self.tree else {
            self.focus_targets.clear();
            self.popover.set_child(Option::<&gtk::Widget>::None);
            self.popover.popdown();
            return;
        };

        self.focus_targets.clear();
        let output_sender = sender.output_sender().clone();
        let renderer = RenderCatalog::with_callback(Rc::new(move |callback: CallbackData| {
            output_sender.emit(ExecPopoverOutput::Callback(callback));
        }));
        match renderer.render(tree) {
            Ok(widget) => {
                self.focus_targets = renderer.focus_targets();
                self.popover.set_child(Some(&widget));
            }
            Err(error) => {
                tracing::warn!(?error, applet = %self.applet_name, "exec applet: failed to render tree");
            }
        }

        self.restore_focus();
    }

    fn remember_current_focus(&mut self) {
        self.last_interacted_id =
            focused_target_id(&self.focus_targets).or(self.last_interacted_id.take());
    }

    fn restore_focus(&self) {
        let Some(id) = restore_target_id(self.last_interacted_id.as_deref(), &self.focus_targets)
        else {
            return;
        };
        let Some(widget) = self.focus_targets.get(&id).cloned() else {
            return;
        };
        glib::idle_add_local_once(move || {
            let _ = widget.grab_focus();
            if let Ok(entry) = widget.clone().downcast::<gtk::Entry>() {
                entry.set_position(-1);
            }
        });
    }
}

fn focused_target_id(focus_targets: &HashMap<String, gtk::Widget>) -> Option<String> {
    focus_targets
        .iter()
        .find_map(|(id, widget)| widget.has_focus().then(|| id.clone()))
}

fn restore_target_id(
    last_interacted_id: Option<&str>,
    focus_targets: &HashMap<String, gtk::Widget>,
) -> Option<String> {
    last_interacted_id
        .filter(|id| focus_targets.contains_key(*id))
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::restore_target_id;

    #[test]
    fn restore_target_id_returns_none_without_interaction() {
        let focus_targets = HashMap::new();
        assert_eq!(restore_target_id(None, &focus_targets), None);
    }
}
