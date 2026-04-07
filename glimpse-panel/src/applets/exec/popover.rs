use std::{collections::HashMap, rc::Rc};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use super::{
    protocol::{CallbackData, HeroData, TreeNode},
    renderer::RenderCatalog,
};

pub struct ExecPopover {
    popover: gtk::Popover,
    hero: Option<HeroData>,
    tree: Option<TreeNode>,
    applet_name: String,
    last_interacted_id: Option<String>,
    focus_targets: HashMap<String, gtk::Widget>,
}

pub struct ExecPopoverInit {
    pub applet_name: String,
}

#[derive(Debug)]
pub enum ExecPopoverInput {
    Clear,
    RememberInteraction(String),
    SetHero(Option<HeroData>),
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

        let model = ExecPopover {
            popover: root.clone(),
            hero: None,
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
                self.hero = None;
                self.tree = None;
                self.last_interacted_id = None;
                self.focus_targets.clear();
                self.popover.set_child(Option::<&gtk::Widget>::None);
                self.popover.popdown();
            }
            ExecPopoverInput::RememberInteraction(id) => {
                self.last_interacted_id = Some(id);
            }
            ExecPopoverInput::SetHero(hero) => {
                self.hero = hero;
                self.rebuild(&sender);
            }
            ExecPopoverInput::SetTree(tree) => {
                self.tree = tree;
                self.rebuild(&sender);
            }
        }
    }
}

impl ExecPopover {
    fn has_content(&self) -> bool {
        self.hero.is_some() || self.tree.is_some()
    }

    fn rebuild(&mut self, sender: &ComponentSender<Self>) {
        self.remember_current_focus();
        if !self.has_content() {
            self.focus_targets.clear();
            self.popover.set_child(Option::<&gtk::Widget>::None);
            self.popover.popdown();
            return;
        }

        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);

        if let Some(hero) = &self.hero {
            outer.append(&build_hero(hero));
        }
        if self.hero.is_some() && self.tree.is_some() {
            outer.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        self.focus_targets.clear();
        if let Some(tree) = &self.tree {
            let output_sender = sender.output_sender().clone();
            let renderer = RenderCatalog::with_callback(Rc::new(move |callback: CallbackData| {
                output_sender.emit(ExecPopoverOutput::Callback(callback));
            }));
            match renderer.render(tree) {
                Ok(widget) => {
                    self.focus_targets = renderer.focus_targets();
                    outer.append(&widget);
                }
                Err(error) => {
                    tracing::warn!(?error, applet = %self.applet_name, "exec applet: failed to render tree");
                }
            }
        }

        self.popover.set_child(Some(&outer));
        self.restore_focus();
    }

    fn remember_current_focus(&mut self) {
        self.last_interacted_id = focused_target_id(&self.focus_targets).or(self.last_interacted_id.take());
    }

    fn restore_focus(&self) {
        let Some(id) = restore_target_id(self.last_interacted_id.as_deref(), &self.focus_targets) else {
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

fn build_hero(hero: &HeroData) -> gtk::Box {
    let hero_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hero_box.add_css_class("exec-hero");

    if let Some(icon) = &hero.icon {
        let image = gtk::Image::new();
        image.set_pixel_size(32);
        super::renderer::apply_icon_to_image(&image, icon);
        hero_box.append(&image);
    }

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text_box.set_valign(gtk::Align::Center);

    let title = gtk::Label::new(Some(&hero.title));
    title.set_halign(gtk::Align::Start);
    title.add_css_class("exec-hero-title");
    text_box.append(&title);

    let subtitle = gtk::Label::new(Some(&hero.subtitle));
    subtitle.set_halign(gtk::Align::Start);
    subtitle.add_css_class("exec-hero-subtitle");
    text_box.append(&subtitle);

    hero_box.append(&text_box);
    hero_box
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

    use relm4::gtk::{self, prelude::*};

    use super::{focused_target_id, restore_target_id};

    #[test]
    fn restore_target_id_requires_existing_widget() {
        if gtk::init().is_err() {
            return;
        }
        let mut focus_targets = HashMap::new();
        focus_targets.insert("version".to_string(), gtk::Entry::new().upcast());

        assert_eq!(
            restore_target_id(Some("version"), &focus_targets).as_deref(),
            Some("version")
        );
        assert_eq!(restore_target_id(Some("missing"), &focus_targets), None);
        assert_eq!(restore_target_id(None, &focus_targets), None);
    }

    #[test]
    fn focused_target_id_prefers_focused_widget() {
        if gtk::init().is_err() {
            return;
        }
        let entry = gtk::Entry::new();
        entry.set_focusable(true);
        let _ = entry.grab_focus();
        let mut focus_targets = HashMap::new();
        focus_targets.insert("version".to_string(), entry.upcast());

        let _ = focused_target_id(&focus_targets);
    }
}
