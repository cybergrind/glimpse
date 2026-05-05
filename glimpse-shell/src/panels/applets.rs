use relm4::{
    Component, ComponentController, Controller,
    gtk::{
        self,
        glib::object::{Cast, CastNone},
        prelude::{BoxExt, WidgetExt},
    },
};
use std::collections::HashMap;

use crate::{
    applets::{
        audio, battery, bluetooth, brightness, clipboard, clock, command, exec, keyboard, mpris,
        network, notifications, pager, privacy, session, tray, weather,
    },
    panels::PanelSection,
    services::framework::Services,
};

pub use glimpse_core::{AppletConfig, AppletType};

fn applet_type_from_name(name: &str) -> Option<AppletType> {
    AppletType::from_config_name(name)
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
    Audio(Controller<audio::Applet>),
    Battery(Controller<battery::Applet>),
    Bluetooth(Controller<bluetooth::Applet>),
    Brightness(Controller<brightness::Applet>),
    Clipboard(Controller<clipboard::Applet>),
    Clock(Controller<clock::Applet>),
    Command(Controller<command::Applet>),
    Exec(Controller<exec::Applet>),
    Keyboard(Controller<keyboard::Applet>),
    Mpris(Controller<mpris::Applet>),
    Network(Controller<network::Applet>),
    Notifications(Controller<notifications::Applet>),
    Pager(Controller<pager::Applet>),
    Privacy(Controller<privacy::Applet>),
    Session(Controller<session::Applet>),
    Tray(Controller<tray::Applet>),
    Weather(Controller<weather::Applet>),
}

impl AppletController {
    pub fn applet_type(&self) -> AppletType {
        match self {
            Self::Audio(_) => AppletType::Audio,
            Self::Battery(_) => AppletType::Battery,
            Self::Bluetooth(_) => AppletType::Bluetooth,
            Self::Brightness(_) => AppletType::Brightness,
            Self::Clipboard(_) => AppletType::Clipboard,
            Self::Clock(_) => AppletType::Clock,
            Self::Command(_) => AppletType::Command,
            Self::Exec(_) => AppletType::Exec,
            Self::Keyboard(_) => AppletType::Keyboard,
            Self::Mpris(_) => AppletType::Mpris,
            Self::Network(_) => AppletType::Network,
            Self::Notifications(_) => AppletType::Notifications,
            Self::Pager(_) => AppletType::Pager,
            Self::Privacy(_) => AppletType::Privacy,
            Self::Session(_) => AppletType::Session,
            Self::Tray(_) => AppletType::Tray,
            Self::Weather(_) => AppletType::Weather,
        }
    }

    pub fn widget(&self) -> gtk::Widget {
        match self {
            Self::Audio(controller) => controller.widget().clone().upcast(),
            Self::Battery(controller) => controller.widget().clone().upcast(),
            Self::Bluetooth(controller) => controller.widget().clone().upcast(),
            Self::Brightness(controller) => controller.widget().clone().upcast(),
            Self::Clipboard(controller) => controller.widget().clone().upcast(),
            Self::Clock(controller) => controller.widget().clone().upcast(),
            Self::Command(controller) => controller.widget().clone().upcast(),
            Self::Exec(controller) => controller.widget().clone().upcast(),
            Self::Keyboard(controller) => controller.widget().clone().upcast(),
            Self::Mpris(controller) => controller.widget().clone().upcast(),
            Self::Network(controller) => controller.widget().clone().upcast(),
            Self::Notifications(controller) => controller.widget().clone().upcast(),
            Self::Pager(controller) => controller.widget().clone().upcast(),
            Self::Privacy(controller) => controller.widget().clone().upcast(),
            Self::Session(controller) => controller.widget().clone().upcast(),
            Self::Tray(controller) => controller.widget().clone().upcast(),
            Self::Weather(controller) => controller.widget().clone().upcast(),
        }
    }

    pub fn reconfigure(&self, config: Option<&AppletConfig>) {
        match self {
            Self::Audio(controller) => {
                controller.emit(audio::Input::Reconfigure(audio::Config::from_raw(
                    &config.cloned(),
                )));
            }
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
            Self::Brightness(controller) => {
                controller.emit(brightness::Input::Reconfigure(
                    brightness::Config::from_raw(&config.cloned()),
                ));
            }
            Self::Clipboard(controller) => {
                controller.emit(clipboard::Input::Reconfigure(clipboard::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Clock(controller) => {
                controller.emit(clock::Input::Reconfigure(clock::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Command(controller) => {
                controller.emit(command::Input::Reconfigure(command::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Exec(controller) => {
                controller.emit(exec::Input::Reconfigure(exec::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Keyboard(controller) => {
                controller.emit(keyboard::Input::Reconfigure(keyboard::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Network(controller) => {
                controller.emit(network::Input::Reconfigure(network::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Mpris(controller) => {
                controller.emit(mpris::Input::Reconfigure(mpris::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Notifications(controller) => {
                controller.emit(notifications::Input::Reconfigure(
                    notifications::Config::from_raw(&config.cloned()),
                ));
            }
            Self::Pager(controller) => {
                controller.emit(pager::Input::Reconfigure(pager::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Privacy(controller) => {
                controller.emit(privacy::Input::Reconfigure(privacy::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Session(controller) => {
                controller.emit(session::Input::Reconfigure(session::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Tray(controller) => {
                controller.emit(tray::Input::Reconfigure(tray::Config::from_raw(
                    &config.cloned(),
                )));
            }
            Self::Weather(controller) => {
                controller.emit(weather::Input::Reconfigure(weather::Config::from_raw(
                    &config.cloned(),
                )));
            }
        }
    }
}

pub fn create_applet(blueprint: AppletBlueprint, services: Services) -> Option<AppletController> {
    match blueprint.applet_type {
        AppletType::Audio => Some(AppletController::Audio(
            audio::Applet::builder()
                .launch(audio::Init {
                    service: services.audio.clone(),
                    config: audio::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
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
        AppletType::Brightness => Some(AppletController::Brightness(
            brightness::Applet::builder()
                .launch(brightness::Init {
                    service: services.brightness.clone(),
                    compositor: services.compositor.clone(),
                    config: brightness::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Clipboard => Some(AppletController::Clipboard(
            clipboard::Applet::builder()
                .launch(clipboard::Init {
                    service: services.clipboard.clone(),
                    config: clipboard::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Clock => Some(AppletController::Clock(
            clock::Applet::builder()
                .launch(clock::Init {
                    clock: services.clock.clone(),
                    calendar: services.calendar_events.clone(),
                    config: clock::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Command => {
            let config = command::Config::from_raw(&blueprint.config);
            if !command::Applet::can_launch(&config) {
                tracing::warn!(name = %blueprint.name, "command applet requires an icon or label");
                return None;
            }
            Some(AppletController::Command(
                command::Applet::builder()
                    .launch(command::Init {
                        name: blueprint.name,
                        config,
                    })
                    .detach(),
            ))
        }
        AppletType::Exec => {
            let config = exec::Config::from_raw(&blueprint.config);
            if !exec::Applet::can_launch(&config) {
                tracing::warn!(name = %blueprint.name, "exec applet requires a non-empty command");
                return None;
            }
            Some(AppletController::Exec(
                exec::Applet::builder()
                    .launch(exec::Init {
                        name: blueprint.name,
                        config,
                    })
                    .detach(),
            ))
        }
        AppletType::Keyboard => Some(AppletController::Keyboard(
            keyboard::Applet::builder()
                .launch(keyboard::Init {
                    service: services.compositor.clone(),
                    config: keyboard::Config::from_raw(&blueprint.config),
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
        AppletType::Mpris => Some(AppletController::Mpris(
            mpris::Applet::builder()
                .launch(mpris::Init {
                    service: services.mpris.clone(),
                    config: mpris::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Notifications => Some(AppletController::Notifications(
            notifications::Applet::builder()
                .launch(notifications::Init {
                    service: services.notifications.clone(),
                    compositor: services.compositor.clone(),
                    config: notifications::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Pager => Some(AppletController::Pager(
            pager::Applet::builder()
                .launch(pager::Init {
                    service: services.compositor.clone(),
                    config: pager::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Privacy => Some(AppletController::Privacy(
            privacy::Applet::builder()
                .launch(privacy::Init {
                    microphone: services.microphone.clone(),
                    webcam: services.webcam.clone(),
                    compositor: services.compositor.clone(),
                    geoclue: services.geoclue.clone(),
                    config: privacy::Config::from_raw(&blueprint.config),
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
        AppletType::Tray => Some(AppletController::Tray(
            tray::Applet::builder()
                .launch(tray::Init {
                    service: services.tray.clone(),
                    config: tray::Config::from_raw(&blueprint.config),
                })
                .detach(),
        )),
        AppletType::Weather => Some(AppletController::Weather(
            weather::Applet::builder()
                .launch(weather::Init {
                    service: services.weather.clone(),
                    config: weather::Config::from_raw(&blueprint.config),
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
        .or_else(|| applet_type_from_name(name));

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
    fn collect_applets_falls_back_to_brightness_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["brightness".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "brightness");
        assert_eq!(entries[0].applet_type, AppletType::Brightness);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_clipboard_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["clipboard".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "clipboard");
        assert_eq!(entries[0].applet_type, AppletType::Clipboard);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_clock_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["clock".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "clock");
        assert_eq!(entries[0].applet_type, AppletType::Clock);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_command_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["command".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "command");
        assert_eq!(entries[0].applet_type, AppletType::Command);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_exec_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["exec".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "exec");
        assert_eq!(entries[0].applet_type, AppletType::Exec);
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
    fn collect_applets_falls_back_to_mpris_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["mpris".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "mpris");
        assert_eq!(entries[0].applet_type, AppletType::Mpris);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_notifications_builtin_name() {
        let entries = collect_applets(
            PanelSection::Left,
            &["notifications".into()],
            &HashMap::new(),
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "notifications");
        assert_eq!(entries[0].applet_type, AppletType::Notifications);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_pager_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["pager".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "pager");
        assert_eq!(entries[0].applet_type, AppletType::Pager);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_falls_back_to_privacy_builtin_name() {
        let entries = collect_applets(PanelSection::Right, &["privacy".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "privacy");
        assert_eq!(entries[0].applet_type, AppletType::Privacy);
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
    fn collect_applets_falls_back_to_tray_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["tray".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "tray");
        assert_eq!(entries[0].applet_type, AppletType::Tray);
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
