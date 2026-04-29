use relm4::{
    Component, ComponentController, Controller,
    gtk::{
        self,
        glib::object::{Cast, CastNone},
        prelude::{BoxExt, WidgetExt},
    },
};
use serde::Deserialize;
use std::collections::HashMap;

use crate::{
    applets::{battery, bluetooth, network, session},
    panels::PanelSection,
    services::framework::Services,
};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AppletType {
    Battery,
    Bluetooth,
    Network,
    Session,
}

impl AppletType {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "battery" => Some(Self::Battery),
            "bluetooth" => Some(Self::Bluetooth),
            "network" => Some(Self::Network),
            "session" => Some(Self::Session),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppletConfig {
    pub extends: Option<AppletType>,
    #[serde(flatten)]
    pub settings: toml::Value,
}

impl Default for AppletConfig {
    fn default() -> Self {
        Self {
            extends: None,
            settings: toml::Value::Table(toml::map::Map::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppletKey {
    pub section: PanelSection,
    pub name: String,
    pub occurrence: usize,
}

#[derive(Debug, Clone)]
pub struct AppletBlueprint {
    pub key: AppletKey,
    pub name: String,
    pub applet_type: AppletType,
    pub config: Option<AppletConfig>,
}

pub enum AppletController {
    Battery(Controller<battery::Applet>),
    Bluetooth(Controller<bluetooth::Applet>),
    Network(Controller<network::Applet>),
    Session(Controller<session::Applet>),
}

impl AppletController {
    pub fn applet_type(&self) -> AppletType {
        match self {
            Self::Battery(_) => AppletType::Battery,
            Self::Bluetooth(_) => AppletType::Bluetooth,
            Self::Network(_) => AppletType::Network,
            Self::Session(_) => AppletType::Session,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        match self {
            Self::Battery(controller) => controller.widget().clone().upcast(),
            Self::Bluetooth(controller) => controller.widget().clone().upcast(),
            Self::Network(controller) => controller.widget().clone().upcast(),
            Self::Session(controller) => controller.widget().clone().upcast(),
        }
    }

    pub fn reconfigure(&self, config: Option<&AppletConfig>) {
        match self {
            Self::Battery(controller) => {
                controller.emit(battery::Input::Reconfigure(battery::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Bluetooth(controller) => {
                controller.emit(bluetooth::Input::Reconfigure(bluetooth::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Network(controller) => {
                controller.emit(network::Input::Reconfigure(network::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Session(controller) => {
                controller.emit(session::Input::Reconfigure(session::Config::from_raw(
                    &config.cloned(),
                )));
            }
        }
    }
}

pub fn create_applet(blueprint: AppletBlueprint, services: Services) -> Option<AppletController> {
    match blueprint.applet_type {
        AppletType::Battery => Some(AppletController::Battery(
            battery::Applet::builder()
                .launch(battery::Init {
                    service: services.battery.clone(),
                    power_service: services.power.clone(),
                    config: battery::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Bluetooth => Some(AppletController::Bluetooth(
            bluetooth::Applet::builder()
                .launch(bluetooth::Init {
                    service: services.bluetooth.clone(),
                    config: bluetooth::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Network => Some(AppletController::Network(
            network::Applet::builder()
                .launch(network::Init {
                    service: services.network.clone(),
                    config: network::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Session => Some(AppletController::Session(
            session::Applet::builder()
                .launch(session::Init {
                    service: services.session.clone(),
                    config: session::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
    }
}

pub fn build_applets(
    section: PanelSection,
    configured_applets: &[String],
    container: &gtk::Box,
    applet_configs: &HashMap<String, AppletConfig>,
    services: Services,
) -> HashMap<AppletKey, AppletController> {
    let mut applets = HashMap::new();
    let entries = collect_applets(section, configured_applets, applet_configs);
    for entry in entries {
        tracing::debug!(name = %entry.name, applet_type = ?entry.applet_type, "create applet");

        if let Some(applet) = create_applet(entry.clone(), services.clone()) {
            let widget = applet.widget();
            container.append(&widget);
            applets.insert(entry.key, applet);
        }
    }

    applets
}

pub fn reconcile_applets(
    section: PanelSection,
    configured_applets: &[String],
    container: &gtk::Box,
    current: &mut HashMap<AppletKey, AppletController>,
    previous_applet_configs: &HashMap<String, AppletConfig>,
    applet_configs: &HashMap<String, AppletConfig>,
    services: Services,
) {
    let current_types = current
        .iter()
        .map(|(key, controller)| (key.clone(), controller.applet_type()))
        .collect();
    let plan = plan_reconcile_applets(
        section,
        configured_applets,
        &current_types,
        previous_applet_configs,
        applet_configs,
    );
    let mut remaining = std::mem::take(current);
    let mut next = HashMap::with_capacity(plan.ordered.len());
    let mut previous_widget: Option<gtk::Widget> = None;

    for planned in plan.ordered {
        let entry = planned.blueprint;
        let controller = match planned.action {
            PlannedAction::Reuse => remaining
                .remove(&entry.key)
                .expect("existing applet missing"),
            PlannedAction::Reconfigure => {
                let existing = remaining
                    .remove(&entry.key)
                    .expect("existing applet missing");
                existing.reconfigure(entry.config.as_ref());
                existing
            }
            PlannedAction::Replace => {
                let existing = remaining
                    .remove(&entry.key)
                    .expect("existing applet missing");
                detach_widget(&existing.widget());
                let Some(created) = create_applet(entry.clone(), services.clone()) else {
                    continue;
                };
                created
            }
            PlannedAction::Create => {
                let Some(created) = create_applet(entry.clone(), services.clone()) else {
                    continue;
                };
                created
            }
        };

        let widget = controller.widget();
        place_widget(container, &widget, previous_widget.as_ref());
        previous_widget = Some(widget);
        next.insert(entry.key, controller);
    }

    for key in plan.removals {
        if let Some(leftover) = remaining.remove(&key) {
            detach_widget(&leftover.widget());
        }
    }

    *current = next;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlannedAction {
    Reuse,
    Reconfigure,
    Replace,
    Create,
}

#[derive(Debug, Clone)]
struct PlannedApplet {
    blueprint: AppletBlueprint,
    action: PlannedAction,
}

#[derive(Debug, Clone)]
struct ReconcilePlan {
    ordered: Vec<PlannedApplet>,
    removals: Vec<AppletKey>,
}

fn plan_reconcile_applets(
    section: PanelSection,
    configured_applets: &[String],
    current_types: &HashMap<AppletKey, AppletType>,
    previous_applet_configs: &HashMap<String, AppletConfig>,
    applet_configs: &HashMap<String, AppletConfig>,
) -> ReconcilePlan {
    let entries = collect_applets(section, configured_applets, applet_configs);
    let mut remaining = current_types.clone();
    let mut ordered = Vec::with_capacity(entries.len());

    for entry in entries {
        let action = match remaining.remove(&entry.key) {
            Some(existing_type) if existing_type == entry.applet_type => {
                if previous_applet_configs.get(&entry.name) != applet_configs.get(&entry.name) {
                    PlannedAction::Reconfigure
                } else {
                    PlannedAction::Reuse
                }
            }
            Some(_) => PlannedAction::Replace,
            None => PlannedAction::Create,
        };

        ordered.push(PlannedApplet {
            blueprint: entry,
            action,
        });
    }

    ReconcilePlan {
        ordered,
        removals: remaining.into_keys().collect(),
    }
}

pub fn collect_applets(
    section: PanelSection,
    configured: &[String],
    applet_configs: &HashMap<String, AppletConfig>,
) -> Vec<AppletBlueprint> {
    let mut name_counts: HashMap<&str, usize> = HashMap::new();

    configured
        .iter()
        .filter_map(|name| {
            let occurrence = name_counts.entry(name.as_str()).or_insert(0);
            let resolved = resolve_applet(section.clone(), name, *occurrence, applet_configs);
            *occurrence += 1;
            resolved
        })
        .collect()
}

fn place_widget(container: &gtk::Box, widget: &gtk::Widget, sibling: Option<&gtk::Widget>) {
    match widget.parent() {
        Some(parent) if parent == container.clone().upcast::<gtk::Widget>() => {
            container.reorder_child_after(widget, sibling);
        }
        Some(_) => {
            detach_widget(widget);
            container.insert_child_after(widget, sibling);
        }
        None => {
            container.insert_child_after(widget, sibling);
        }
    }
}

fn detach_widget(widget: &gtk::Widget) {
    if let Some(parent_box) = widget.parent().and_downcast::<gtk::Box>() {
        parent_box.remove(widget);
    }
}

fn resolve_applet(
    section: PanelSection,
    name: &str,
    occurrence: usize,
    applet_configs: &HashMap<String, AppletConfig>,
) -> Option<AppletBlueprint> {
    let applet_config = applet_configs.get(name).cloned();
    let applet_type = applet_config
        .as_ref()
        .and_then(|config| config.extends)
        .or_else(|| AppletType::from_name(name));

    let Some(applet_type) = applet_type else {
        tracing::warn!(name, "unknown applet config, ignoring");
        return None;
    };

    let key = AppletKey {
        section,
        name: name.to_string(),
        occurrence,
    };

    Some(AppletBlueprint {
        key,
        name: name.to_string(),
        applet_type,
        config: applet_config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_applets_uses_named_config_entry() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert(
            "laptop".to_string(),
            AppletConfig {
                extends: Some(AppletType::Battery),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        );

        let entries = collect_applets(PanelSection::Right, &["laptop".into()], &applet_configs);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "laptop");
        assert_eq!(entries[0].applet_type, AppletType::Battery);
        assert!(entries[0].config.is_some());
    }

    #[test]
    fn collect_applets_falls_back_to_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["battery".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "battery");
        assert_eq!(entries[0].applet_type, AppletType::Battery);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_bluetooth_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["bluetooth".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "bluetooth");
        assert_eq!(entries[0].applet_type, AppletType::Bluetooth);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_network_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["network".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "network");
        assert_eq!(entries[0].applet_type, AppletType::Network);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_session_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["session".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "session");
        assert_eq!(entries[0].applet_type, AppletType::Session);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_uses_builtin_type_for_named_builtin_config_without_extends() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert("battery".to_string(), AppletConfig::default());

        let entries = collect_applets(PanelSection::Left, &["battery".into()], &applet_configs);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "battery");
        assert_eq!(entries[0].applet_type, AppletType::Battery);
        assert!(entries[0].config.is_some());
        assert_eq!(entries[0].config.as_ref().unwrap().extends, None);
    }

    #[test]
    fn collect_applets_ignores_unknown_named_config_without_extends() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert("custom_battery".to_string(), AppletConfig::default());

        let entries = collect_applets(
            PanelSection::Right,
            &["custom_battery".into()],
            &applet_configs,
        );

        assert!(entries.is_empty());
    }

    #[test]
    fn collect_applets_assigns_stable_occurrence_keys_for_duplicates() {
        let entries = collect_applets(
            PanelSection::Left,
            &[
                "battery".into(),
                "custom".into(),
                "battery".into(),
                "battery".into(),
            ],
            &HashMap::from([(
                "custom".into(),
                AppletConfig {
                    extends: Some(AppletType::Battery),
                    settings: toml::Value::Table(toml::map::Map::new()),
                },
            )]),
        );

        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].key.occurrence, 0);
        assert_eq!(entries[1].key.occurrence, 0);
        assert_eq!(entries[2].key.occurrence, 1);
        assert_eq!(entries[3].key.occurrence, 2);
    }

    #[test]
    fn collect_applets_keeps_duplicate_keys_stable_when_inserting_before_them() {
        let old = collect_applets(
            PanelSection::Left,
            &["battery".into(), "battery".into()],
            &HashMap::new(),
        );
        let new = collect_applets(
            PanelSection::Left,
            &["custom".into(), "battery".into(), "battery".into()],
            &HashMap::from([(
                "custom".into(),
                AppletConfig {
                    extends: Some(AppletType::Battery),
                    settings: toml::Value::Table(toml::map::Map::new()),
                },
            )]),
        );

        assert_eq!(old[0].key, new[1].key);
        assert_eq!(old[1].key, new[2].key);
    }

    #[test]
    fn plan_reconcile_reuses_duplicates_when_inserting_before_them() {
        let current_entries = collect_applets(
            PanelSection::Left,
            &["battery".into(), "battery".into()],
            &HashMap::new(),
        );
        let current_types = current_entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.applet_type))
            .collect();

        let new_configs = HashMap::from([(
            "custom".into(),
            AppletConfig {
                extends: Some(AppletType::Battery),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        )]);
        let plan = plan_reconcile_applets(
            PanelSection::Left,
            &["custom".into(), "battery".into(), "battery".into()],
            &current_types,
            &HashMap::new(),
            &new_configs,
        );

        assert_eq!(plan.ordered.len(), 3);
        assert_eq!(plan.ordered[0].blueprint.name, "custom");
        assert_eq!(plan.ordered[0].action, PlannedAction::Create);
        assert_eq!(plan.ordered[1].blueprint.key, current_entries[0].key);
        assert_eq!(plan.ordered[1].action, PlannedAction::Reuse);
        assert_eq!(plan.ordered[2].blueprint.key, current_entries[1].key);
        assert_eq!(plan.ordered[2].action, PlannedAction::Reuse);
        assert!(plan.removals.is_empty());
    }

    #[test]
    fn plan_reconcile_marks_named_applet_for_reconfigure_on_config_change() {
        let current_entries = collect_applets(
            PanelSection::Left,
            &["battery".into()],
            &HashMap::from([("battery".into(), AppletConfig::default())]),
        );
        let current_types = current_entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.applet_type))
            .collect();
        let previous_configs = HashMap::from([("battery".into(), AppletConfig::default())]);
        let next_configs = HashMap::from([(
            "battery".into(),
            AppletConfig {
                extends: None,
                settings: toml::Value::Table(toml::map::Map::from_iter([(
                    "show_icon".into(),
                    toml::Value::Boolean(false),
                )])),
            },
        )]);

        let plan = plan_reconcile_applets(
            PanelSection::Left,
            &["battery".into()],
            &current_types,
            &previous_configs,
            &next_configs,
        );

        assert_eq!(plan.ordered.len(), 1);
        assert_eq!(plan.ordered[0].blueprint.key, current_entries[0].key);
        assert_eq!(plan.ordered[0].action, PlannedAction::Reconfigure);
        assert!(plan.removals.is_empty());
    }

    #[test]
    fn plan_reconcile_removes_obsolete_applets() {
        let applet_configs = HashMap::from([(
            "custom".into(),
            AppletConfig {
                extends: Some(AppletType::Battery),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        )]);
        let current_entries = collect_applets(
            PanelSection::Left,
            &["battery".into(), "custom".into()],
            &applet_configs,
        );
        let current_types = current_entries
            .iter()
            .map(|entry| (entry.key.clone(), entry.applet_type))
            .collect();

        let plan = plan_reconcile_applets(
            PanelSection::Left,
            &["battery".into()],
            &current_types,
            &applet_configs,
            &HashMap::new(),
        );

        assert_eq!(plan.ordered.len(), 1);
        assert_eq!(plan.ordered[0].blueprint.key, current_entries[0].key);
        assert_eq!(plan.ordered[0].action, PlannedAction::Reuse);
        assert_eq!(plan.removals, vec![current_entries[1].key.clone()]);
    }
}
