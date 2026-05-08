#![allow(unused_assignments)]

use std::collections::{HashMap, HashSet};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, gio, prelude::*},
};

use crate::components::{
    animated_popover::AnimatedPopover, hero::HeroView, popover_scroll, popover_shell::PopoverShell,
};
use glimpse_core::services::clipboard::{ClipboardEntry, Command, State};

use super::{
    components::{HistoryRow, HistoryRowInit},
    format,
};

pub struct Popover {
    animation: AnimatedPopover,
    state: State,
    rows: HashMap<u64, ClipboardHistoryRow>,
    list: gtk::Box,
    scroller: gtk::ScrolledWindow,
    empty: gtk::Box,
    hero_icon: gtk::Image,
    hero_subtitle: gtk::Label,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    Select(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Opened,
    Closed,
    Command(Command),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "clipboard-popover",
            add_css_class: "popover-size-medium",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                content {
                    #[name = "hero"]
                    #[template]
                    HeroView {},

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[name = "empty"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_vexpand: true,
                        set_hexpand: true,
                        add_css_class: "empty-state",

                        gtk::Label {
                            add_css_class: "empty-state__title",
                            set_label: "No clipboard items",
                        },

                        gtk::Label {
                            add_css_class: "empty-state__subtitle",
                            set_label: "Copy something to start history.",
                        },
                    },

                    #[name = "scroller"]
                    gtk::ScrolledWindow {
                        set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                        set_vexpand: true,
                        set_propagate_natural_height: true,

                        #[name = "list"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 2,
                            add_css_class: "clipboard-list",
                        },
                    },
                },

                #[template_child]
                footer {
                    set_visible: false,
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        popover_scroll::install_half_monitor_limit(&widgets.root, &widgets.scroller, &init.parent);

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(PopoverOutput::Opened);
        });

        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(PopoverOutput::Closed);
        });

        widgets.hero.icon.set_icon_name(Some("edit-paste-symbolic"));
        widgets.hero.title.set_label("Clipboard");
        widgets.hero.subtitle.set_label("No items");

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            state: State::default(),
            rows: HashMap::new(),
            list: widgets.list.clone(),
            scroller: widgets.scroller.clone(),
            empty: widgets.empty.clone(),
            hero_icon: widgets.hero.icon.clone(),
            hero_subtitle: widgets.hero.subtitle.clone(),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.animation.toggle(),
            PopoverInput::UpdateState(state) => {
                self.state = state;
                self.sync(&sender);
            }
            PopoverInput::Select(id) => {
                let _ = sender.output(PopoverOutput::Command(Command::Select(id)));
            }
        }
    }
}

impl Popover {
    fn sync(&mut self, sender: &ComponentSender<Self>) {
        let mut seen = HashSet::new();
        let mut previous: Option<gtk::Widget> = None;

        self.hero_icon
            .set_icon_name(Some(format::icon_name(&self.state)));
        self.hero_subtitle
            .set_label(&format::hero_subtitle(&self.state));

        for entry in &self.state.history {
            seen.insert(entry.id);
            let row = self
                .rows
                .entry(entry.id)
                .or_insert_with(|| ClipboardHistoryRow::new(entry.id, sender));
            row.update(entry);
            place_row(row, &self.list, previous.as_ref());
            previous = Some(row.root.as_ref().clone().upcast());
        }

        let has_items = !self.state.history.is_empty();
        self.empty.set_visible(!has_items);
        self.scroller.set_visible(has_items);

        self.rows.retain(|id, row| {
            let keep = seen.contains(id);
            if !keep {
                if let Some(parent) = row.root.as_ref().parent() {
                    if let Ok(parent) = parent.downcast::<gtk::Box>() {
                        parent.remove(row.root.as_ref());
                    }
                }
            }
            keep
        });
    }
}

struct ClipboardHistoryRow {
    root: HistoryRow,
    _context_menu: gtk::PopoverMenu,
}

impl ClipboardHistoryRow {
    fn new(id: u64, sender: &ComponentSender<Popover>) -> Self {
        let root = HistoryRow::init(HistoryRowInit {
            icon: "edit-paste-symbolic",
            preview: String::new(),
        });

        root.button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(PopoverInput::Select(id))
        });

        let context_menu = build_row_context_menu(&root, id, sender);

        Self {
            root,
            _context_menu: context_menu,
        }
    }

    fn update(&self, entry: &ClipboardEntry) {
        self.root
            .icon
            .set_icon_name(Some(format::entry_icon(entry)));
        self.root.preview.set_label(&entry.preview);
    }
}

fn build_row_context_menu(
    row: &HistoryRow,
    id: u64,
    sender: &ComponentSender<Popover>,
) -> gtk::PopoverMenu {
    let action_group = gio::SimpleActionGroup::new();

    let remove_action = gio::SimpleAction::new("remove", None);
    remove_action.connect_activate({
        let sender = sender.clone();
        move |_, _| {
            let _ = sender.output(PopoverOutput::Command(Command::Remove(id)));
        }
    });
    action_group.add_action(&remove_action);

    row.as_ref()
        .insert_action_group("clipboard-row", Some(&action_group));

    let menu = gio::Menu::new();
    menu.append(Some("Remove"), Some("clipboard-row.remove"));

    let context_menu = gtk::PopoverMenu::from_model(Some(&menu));
    context_menu.add_css_class("clipboard-row-menu");
    context_menu.set_parent(row.as_ref());
    context_menu.set_has_arrow(false);

    let click = gtk::GestureClick::new();
    click.set_button(3);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_pressed({
        let context_menu = context_menu.clone();
        move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            context_menu.popup();
        }
    });
    row.as_ref().add_controller(click);

    row.as_ref().connect_destroy({
        let context_menu = context_menu.clone();
        move |_| context_menu.unparent()
    });

    context_menu
}

fn place_row(row: &ClipboardHistoryRow, container: &gtk::Box, previous: Option<&gtk::Widget>) {
    let row_widget = row.root.as_ref();
    let target = container.clone().upcast::<gtk::Widget>();
    let already_in_container = row_widget.parent().is_some_and(|parent| parent == target);

    if !already_in_container {
        if let Some(parent) = row_widget.parent() {
            if let Ok(parent) = parent.downcast::<gtk::Box>() {
                parent.remove(row_widget);
            }
        }
        container.append(row_widget);
    }
    container.reorder_child_after(row_widget, previous);
}
