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

use crate::{applets::battery, panels::PanelSection, services::framework::Services};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AppletType {
    Battery,
}

impl AppletType {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "battery" => Some(Self::Battery),
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
}

impl AppletController {
    pub fn applet_type(&self) -> AppletType {
        match self {
            Self::Battery(_) => AppletType::Battery,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        match self {
            Self::Battery(controller) => controller.widget().clone().upcast(),
        }
    }

    pub fn reconfigure(&self, config: Option<&AppletConfig>) {
        match self {
            Self::Battery(controller) => {
                controller.emit(battery::Input::Reconfigure(battery::Config::from_raw(
                    &config.cloned(),
                )));
            }
        }
    }
}

pub fn create_applet(blueprint: AppletBlueprint, services: Services) -> Option<AppletController> {
    let _ = services;

    match blueprint.applet_type {
        AppletType::Battery => Some(AppletController::Battery(
            battery::Applet::builder()
                .launch(battery::Init {
                    config: battery::Config::from_raw(&blueprint.config),
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
    let entries = collect_applets(section, configured_applets, applet_configs);
    let mut remaining = std::mem::take(current);
    let mut next = HashMap::with_capacity(entries.len());
    let mut previous_widget: Option<gtk::Widget> = None;

    for entry in entries {
        let controller = match remaining.remove(&entry.key) {
            Some(existing) if existing.applet_type() == entry.applet_type => {
                if previous_applet_configs.get(&entry.name) != applet_configs.get(&entry.name) {
                    existing.reconfigure(entry.config.as_ref());
                }
                existing
            }
            Some(existing) => {
                detach_widget(&existing.widget());
                let Some(created) = create_applet(entry.clone(), services.clone()) else {
                    continue;
                };
                created
            }
            None => {
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

    for leftover in remaining.into_values() {
        detach_widget(&leftover.widget());
    }

    *current = next;
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
}
