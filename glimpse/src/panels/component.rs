use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, glib, prelude::*},
};

use std::collections::HashMap;

use crate::{
    applets::{AppletController, create_applet, reconfigure_applet},
    panels::diff::{AppletInstanceKey, PanelKey, PanelSection, build_section_entries},
    services::ServicesHandle,
};
use glimpse::config::{AppletConfig, PanelConfig, PanelPosition};

pub struct Panel {
    panel_key: PanelKey,
    config: PanelConfig,
    applet_configs: HashMap<String, AppletConfig>,
    dbus: zbus::Connection,
    system: zbus::Connection,
    services: ServicesHandle,
    revealer: gtk::Revealer,
    left_box: gtk::Box,
    center_box: gtk::Box,
    right_box: gtk::Box,
    left_applets: SectionInstances,
    center_applets: SectionInstances,
    right_applets: SectionInstances,
}

pub struct Init {
    pub panel_key: PanelKey,
    pub config: PanelConfig,
    pub dbus: zbus::Connection,
    pub system: zbus::Connection,
    pub services: ServicesHandle,
    pub applet_configs: HashMap<String, AppletConfig>,
}

pub struct PanelRuntimeConfig {
    pub panel_key: PanelKey,
    pub config: PanelConfig,
    pub dbus: zbus::Connection,
    pub system: zbus::Connection,
    pub services: ServicesHandle,
    pub applet_configs: HashMap<String, AppletConfig>,
}

struct PanelAppletInstance {
    controller: AppletController,
}

type SectionInstances = HashMap<AppletInstanceKey, PanelAppletInstance>;

pub enum Input {
    Reconfigure(PanelRuntimeConfig),
}

impl std::fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Input::Reconfigure(_) => f.write_str("PanelInput::Reconfigure(..)"),
        }
    }
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl Component for Panel {
    type Init = Init;
    type Input = Input;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Window {
            set_decorated: false,

            #[local_ref]
            revealer -> gtk::Revealer {
                #[local_ref]
                layout -> gtk::CenterBox {}
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        tracing::info!(
            "configuring panel, position {:?}, {} applets",
            init.config.position,
            init.config.left.len() + init.config.center.len() + init.config.right.len()
        );

        Self::init_layer_shell(&root);
        Self::apply_window_config(&root, &init.config);
        root.add_css_class("panel");

        let left_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        left_box.set_halign(gtk::Align::Start);

        let center_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        center_box.set_halign(gtk::Align::Center);

        let right_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        right_box.set_halign(gtk::Align::End);

        let layout = gtk::CenterBox::new();
        let revealer = gtk::Revealer::new();
        revealer.set_transition_duration(180);
        revealer.set_transition_type(reveal_transition_for_position(init.config.position.clone()));
        revealer.set_reveal_child(false);

        let left_applets = build_section_applets(
            &left_box,
            &init.panel_key,
            PanelSection::Left,
            &init.config.left,
            &init.applet_configs,
            init.dbus.clone(),
            init.system.clone(),
            init.services.clone(),
        );
        let center_applets = build_section_applets(
            &center_box,
            &init.panel_key,
            PanelSection::Center,
            &init.config.center,
            &init.applet_configs,
            init.dbus.clone(),
            init.system.clone(),
            init.services.clone(),
        );
        let right_applets = build_section_applets(
            &right_box,
            &init.panel_key,
            PanelSection::Right,
            &init.config.right,
            &init.applet_configs,
            init.dbus.clone(),
            init.system.clone(),
            init.services.clone(),
        );

        let model = Panel {
            panel_key: init.panel_key,
            config: init.config,
            applet_configs: init.applet_configs,
            dbus: init.dbus,
            system: init.system,
            services: init.services,
            revealer: revealer.clone(),
            left_box: left_box.clone(),
            center_box: center_box.clone(),
            right_box: right_box.clone(),
            left_applets,
            center_applets,
            right_applets,
        };
        layout.set_hexpand(true);
        layout.set_start_widget(Some(&left_box));
        layout.set_center_widget(Some(&center_box));
        layout.set_end_widget(Some(&right_box));
        let widgets = view_output!();
        root.present();
        let revealer_clone = revealer.clone();
        glib::idle_add_local_once(move || {
            revealer_clone.set_reveal_child(true);
        });
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            Input::Reconfigure(runtime) => {
                let old_applet_configs = self.applet_configs.clone();
                let next_config = runtime.config;
                let next_applet_configs = runtime.applet_configs;

                if section_needs_reconcile(
                    &self.config.left,
                    &next_config.left,
                    &old_applet_configs,
                    &next_applet_configs,
                ) {
                    reconcile_section(
                        &self.left_box,
                        &mut self.left_applets,
                        build_section_entries(
                            &runtime.panel_key,
                            PanelSection::Left,
                            &next_config.left,
                            &next_applet_configs,
                        ),
                        &old_applet_configs,
                        &next_applet_configs,
                        self.dbus.clone(),
                        self.system.clone(),
                        self.services.clone(),
                    );
                }
                if section_needs_reconcile(
                    &self.config.center,
                    &next_config.center,
                    &old_applet_configs,
                    &next_applet_configs,
                ) {
                    reconcile_section(
                        &self.center_box,
                        &mut self.center_applets,
                        build_section_entries(
                            &runtime.panel_key,
                            PanelSection::Center,
                            &next_config.center,
                            &next_applet_configs,
                        ),
                        &old_applet_configs,
                        &next_applet_configs,
                        self.dbus.clone(),
                        self.system.clone(),
                        self.services.clone(),
                    );
                }
                if section_needs_reconcile(
                    &self.config.right,
                    &next_config.right,
                    &old_applet_configs,
                    &next_applet_configs,
                ) {
                    reconcile_section(
                        &self.right_box,
                        &mut self.right_applets,
                        build_section_entries(
                            &runtime.panel_key,
                            PanelSection::Right,
                            &next_config.right,
                            &next_applet_configs,
                        ),
                        &old_applet_configs,
                        &next_applet_configs,
                        self.dbus.clone(),
                        self.system.clone(),
                        self.services.clone(),
                    );
                }

                self.panel_key = runtime.panel_key;
                self.config = next_config;
                self.applet_configs = next_applet_configs;
                self.dbus = runtime.dbus;
                self.system = runtime.system;
                self.services = runtime.services;
                self.revealer
                    .set_transition_type(reveal_transition_for_position(
                        self.config.position.clone(),
                    ));
                self.revealer.set_reveal_child(true);
                Self::apply_window_config(root, &self.config);
            }
        }
    }
}

fn reveal_transition_for_position(position: PanelPosition) -> gtk::RevealerTransitionType {
    match position {
        PanelPosition::Top => gtk::RevealerTransitionType::SlideDown,
        PanelPosition::Bottom => gtk::RevealerTransitionType::SlideUp,
    }
}

impl Panel {
    fn init_layer_shell(window: &gtk::Window) {
        window.init_layer_shell();
        window.set_layer(Layer::Top);
        window.set_namespace("glimpse-panel");
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
        window.auto_exclusive_zone_enable();
    }

    fn apply_window_config(window: &gtk::Window, config: &PanelConfig) {
        window.set_height_request(config.height);
        window.set_margin(Edge::Left, config.margin.left);
        window.set_margin(Edge::Right, config.margin.right);
        window.set_margin(Edge::Top, config.margin.top);
        window.set_margin(Edge::Bottom, config.margin.bottom);
        window.set_anchor(Edge::Top, false);
        window.set_anchor(Edge::Bottom, false);
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, false);

        match config.position {
            PanelPosition::Top => {
                window.set_anchor(Edge::Top, true);
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Right, true);
            }
            PanelPosition::Bottom => {
                window.set_anchor(Edge::Bottom, true);
                window.set_anchor(Edge::Left, true);
                window.set_anchor(Edge::Right, true);
            }
        }
    }
}

fn build_section_applets(
    container: &gtk::Box,
    panel: &PanelKey,
    section: PanelSection,
    names: &[String],
    applet_configs: &HashMap<String, AppletConfig>,
    dbus: zbus::Connection,
    system: zbus::Connection,
    services: ServicesHandle,
) -> SectionInstances {
    let mut applets = HashMap::new();
    let entries = build_section_entries(panel, section, names, applet_configs);

    for entry in entries {
        let config = applet_configs.get(&entry.name);
        tracing::debug!(
            name = %entry.name,
            applet_type = %entry.applet_type,
            "create applet"
        );
        if let Some(applet) = create_applet(
            config,
            &entry.name,
            dbus.clone(),
            system.clone(),
            services.clone(),
        ) {
            container.append(&applet.widget());
            applets.insert(entry.key, PanelAppletInstance { controller: applet });
        }
    }

    applets
}

fn reconcile_section(
    container: &gtk::Box,
    current: &mut SectionInstances,
    next_entries: Vec<crate::panels::diff::SectionEntry>,
    old_applet_configs: &HashMap<String, AppletConfig>,
    new_applet_configs: &HashMap<String, AppletConfig>,
    dbus: zbus::Connection,
    system: zbus::Connection,
    services: ServicesHandle,
) {
    clear_section_container(container);

    let mut remaining = std::mem::take(current);
    let mut next_instances = HashMap::with_capacity(next_entries.len());

    for entry in next_entries {
        let new_config = new_applet_configs.get(&entry.name);

        if let Some(existing) = remaining.remove(&entry.key) {
            let old_config = old_applet_configs.get(&entry.name);
            let should_reuse = if old_config == new_config {
                true
            } else {
                let outcome = reconfigure_applet(
                    &existing.controller,
                    crate::applets::registry::AppletReconfigureRequest {
                        applet_config: new_config,
                        name: &entry.name,
                    },
                );
                !outcome.needs_recreate()
            };

            if should_reuse {
                container.append(&existing.controller.widget());
                next_instances.insert(entry.key, existing);
                continue;
            }

            detach_applet(container, &existing.controller);
        }

        if let Some(applet) = create_applet(
            new_config,
            &entry.name,
            dbus.clone(),
            system.clone(),
            services.clone(),
        ) {
            container.append(&applet.widget());
            next_instances.insert(entry.key, PanelAppletInstance { controller: applet });
        }
    }

    for leftover in remaining.into_values() {
        detach_applet(container, &leftover.controller);
    }

    *current = next_instances;
}

fn detach_applet(container: &gtk::Box, controller: &AppletController) {
    let widget = controller.widget();
    if widget.parent().as_ref() == Some(container.upcast_ref()) {
        container.remove(&widget);
    }
}

fn clear_section_container(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn section_needs_reconcile(
    old_names: &[String],
    new_names: &[String],
    old_applet_configs: &HashMap<String, AppletConfig>,
    new_applet_configs: &HashMap<String, AppletConfig>,
) -> bool {
    old_names != new_names
        || !configs_match_for_names(new_names, old_applet_configs, new_applet_configs)
}

fn configs_match_for_names(
    names: &[String],
    old_applet_configs: &HashMap<String, AppletConfig>,
    new_applet_configs: &HashMap<String, AppletConfig>,
) -> bool {
    names
        .iter()
        .all(|name| old_applet_configs.get(name) == new_applet_configs.get(name))
}

#[cfg(test)]
mod tests {
    use super::reveal_transition_for_position;
    use glimpse::config::PanelPosition;
    use relm4::gtk;

    #[test]
    fn top_panels_reveal_from_top_edge() {
        assert_eq!(
            reveal_transition_for_position(PanelPosition::Top),
            gtk::RevealerTransitionType::SlideDown
        );
    }

    #[test]
    fn bottom_panels_reveal_from_bottom_edge() {
        assert_eq!(
            reveal_transition_for_position(PanelPosition::Bottom),
            gtk::RevealerTransitionType::SlideUp
        );
    }
}
