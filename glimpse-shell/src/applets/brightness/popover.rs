#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    applets::brightness::{
        components::{
            SourceControl, SourceControlInput, SourceSection, SourceSectionInit, SourceSectionInput,
        },
        format,
    },
    components::{
        animated_popover::AnimatedPopover, hero::HeroView, popover_scroll,
        popover_shell::PopoverShell,
    },
};
use glimpse_core::services::brightness::{BrightnessSource, BrightnessSourceKind, Command, State};

pub struct Popover {
    animation: AnimatedPopover,
    state: State,
    hero_icon_name: String,
    hero_subtitle: String,
    primary_source: Option<BrightnessSource>,
    primary_control: Controller<SourceControl>,
    displays: Controller<SourceSection>,
    keyboard: Controller<SourceSection>,
    other: Controller<SourceSection>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    SectionCommand(Command),
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
            add_css_class: "brightness-popover",
            add_css_class: "popover-size-medium",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[name = "hero"]
                    #[template]
                    HeroView {},

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[local_ref]
                    primary_widget -> gtk::Grid {},

                    #[name = "sections_separator"]
                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[name = "scroller"]
                    gtk::ScrolledWindow {
                        set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                        set_vexpand: false,
                        set_propagate_natural_height: true,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 8,

                            #[local_ref]
                            displays_widget -> gtk::Box {},

                            #[local_ref]
                            keyboard_widget -> gtk::Box {},

                            #[local_ref]
                            other_widget -> gtk::Box {},
                        },
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let primary_control = SourceControl::builder()
            .launch(empty_source("brightness:primary"))
            .forward(sender.input_sender(), PopoverInput::SectionCommand);
        let primary_widget = primary_control.widget().clone();

        let displays = SourceSection::builder()
            .launch(SourceSectionInit {
                sources: Vec::new(),
            })
            .forward(sender.input_sender(), PopoverInput::SectionCommand);
        let displays_widget = displays.widget().clone();

        let keyboard = SourceSection::builder()
            .launch(SourceSectionInit {
                sources: Vec::new(),
            })
            .forward(sender.input_sender(), PopoverInput::SectionCommand);
        let keyboard_widget = keyboard.widget().clone();

        let other = SourceSection::builder()
            .launch(SourceSectionInit {
                sources: Vec::new(),
            })
            .forward(sender.input_sender(), PopoverInput::SectionCommand);
        let other_widget = other.widget().clone();

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

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            state: State::default(),
            hero_icon_name: "display-brightness-symbolic".into(),
            hero_subtitle: "No brightness controls".into(),
            primary_source: None,
            primary_control,
            displays,
            keyboard,
            other,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => {
                self.animation.toggle();
            }
            PopoverInput::UpdateState(state) => {
                self.hero_icon_name = format::icon_name(&state).into();
                self.hero_subtitle = format::hero_subtitle(&state);
                self.primary_source = format::primary_source(&state).cloned();
                if let Some(source) = &self.primary_source {
                    self.primary_control
                        .emit(SourceControlInput::Update(source.clone()));
                }

                self.displays
                    .emit(SourceSectionInput::Update(filter_sources(
                        &state.sources,
                        |kind| {
                            matches!(
                                kind,
                                BrightnessSourceKind::BuiltInDisplay
                                    | BrightnessSourceKind::ExternalDisplay
                            )
                        },
                    )));
                self.keyboard
                    .emit(SourceSectionInput::Update(filter_sources(
                        &state.sources,
                        |kind| kind == BrightnessSourceKind::Keyboard,
                    )));
                self.other.emit(SourceSectionInput::Update(filter_sources(
                    &state.sources,
                    |kind| kind == BrightnessSourceKind::Other,
                )));

                self.state = state;
            }
            PopoverInput::SectionCommand(command) => {
                let _ = sender.output(PopoverOutput::Command(command));
            }
        }
    }

    fn post_view() {
        hero.icon.set_icon_name(Some(&model.hero_icon_name));
        hero.title.set_label("Brightness");
        hero.subtitle.set_label(&model.hero_subtitle);
        let has_secondary = has_secondary_sources(&model.state);
        primary_widget.set_visible(model.primary_source.is_some());
        sections_separator.set_visible(has_secondary);
        scroller.set_visible(has_secondary);
    }
}

fn filter_sources(
    sources: &[BrightnessSource],
    include: impl Fn(BrightnessSourceKind) -> bool,
) -> Vec<BrightnessSource> {
    sources
        .iter()
        .filter(|source| source.is_usable() && !source.primary && include(source.kind))
        .cloned()
        .collect()
}

fn has_secondary_sources(state: &State) -> bool {
    state
        .sources
        .iter()
        .any(|source| source.is_usable() && !source.primary)
}

fn empty_source(id: &str) -> BrightnessSource {
    BrightnessSource {
        id: id.into(),
        name: "Brightness".into(),
        kind: BrightnessSourceKind::BuiltInDisplay,
        icon: "display-brightness-symbolic".into(),
        current: 0,
        max: 100,
        percent: 0,
        writable: false,
        primary: true,
        available: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_sources_groups_displays_together() {
        let sources = vec![
            source("backlight", BrightnessSourceKind::BuiltInDisplay),
            source("ddc", BrightnessSourceKind::ExternalDisplay),
            source("kbd", BrightnessSourceKind::Keyboard),
        ];

        let displays = filter_sources(&sources, |kind| {
            matches!(
                kind,
                BrightnessSourceKind::BuiltInDisplay | BrightnessSourceKind::ExternalDisplay
            )
        });

        assert_eq!(displays.len(), 2);
    }

    #[test]
    fn filter_sources_hides_disabled_sources() {
        let mut disabled = source("disabled", BrightnessSourceKind::BuiltInDisplay);
        disabled.writable = false;
        let sources = vec![
            disabled,
            source("enabled", BrightnessSourceKind::BuiltInDisplay),
        ];

        let displays = filter_sources(&sources, |kind| {
            kind == BrightnessSourceKind::BuiltInDisplay
        });

        assert_eq!(displays.len(), 1);
        assert_eq!(displays[0].id, "enabled");
    }

    #[test]
    fn filter_sources_excludes_primary_source() {
        let mut primary = source("primary", BrightnessSourceKind::BuiltInDisplay);
        primary.primary = true;
        let sources = vec![
            primary,
            source("secondary", BrightnessSourceKind::ExternalDisplay),
        ];

        let displays = filter_sources(&sources, |kind| {
            matches!(
                kind,
                BrightnessSourceKind::BuiltInDisplay | BrightnessSourceKind::ExternalDisplay
            )
        });

        assert_eq!(displays.len(), 1);
        assert_eq!(displays[0].id, "secondary");
    }

    fn source(id: &str, kind: BrightnessSourceKind) -> BrightnessSource {
        BrightnessSource {
            id: id.into(),
            name: id.into(),
            kind,
            icon: "display-brightness-symbolic".into(),
            current: 50,
            max: 100,
            percent: 50,
            writable: true,
            primary: false,
            available: true,
        }
    }
}
