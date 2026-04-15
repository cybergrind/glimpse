use adw::prelude::GtkWindowExt;
use relm4::{Component, ComponentController, ComponentSender, Controller};

use glimpse::{
    config::{Config, PanelConfig},
    notifications::NotificationsServiceHandle,
};

use crate::{
    app::App,
    applets::notifications::{
        NotificationPopup, NotificationPopupInit, NotificationPopupInput, NotificationsConfig,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum PopupSyncPlan {
    Keep,
    Create(NotificationsConfig),
    Update(NotificationsConfig),
    Remove,
}

pub(super) fn setup_notification_popup(
    config: &Config,
    service: NotificationsServiceHandle,
    sender: ComponentSender<App>,
) -> Option<Controller<NotificationPopup>> {
    let popup_config = notifications_popup_config(config)?;
    Some(
        NotificationPopup::builder()
            .launch(NotificationPopupInit {
                config: popup_config,
                service,
            })
            .forward(
                sender.input_sender(),
                crate::app::Input::NotificationCommand,
            ),
    )
}

pub(super) fn sync_notification_popup(
    old_config: &Config,
    new_config: &Config,
    notification_popup: &mut Option<Controller<NotificationPopup>>,
    service: NotificationsServiceHandle,
    sender: ComponentSender<App>,
) {
    match popup_sync_plan(
        notifications_popup_config(old_config),
        notifications_popup_config(new_config),
    ) {
        PopupSyncPlan::Keep => {}
        PopupSyncPlan::Create(config) => {
            *notification_popup = Some(
                NotificationPopup::builder()
                    .launch(NotificationPopupInit { config, service })
                    .forward(
                        sender.input_sender(),
                        crate::app::Input::NotificationCommand,
                    ),
            );
        }
        PopupSyncPlan::Update(config) => {
            if let Some(popup) = notification_popup {
                popup.emit(NotificationPopupInput::Reconfigure(config));
            }
        }
        PopupSyncPlan::Remove => {
            if let Some(popup) = notification_popup.take() {
                popup.widget().close();
            }
        }
    }
}

fn notifications_popup_config(config: &Config) -> Option<NotificationsConfig> {
    for panel in &config.panels {
        for name in panel_applet_names(panel) {
            let applet_config = config.applets.get(name);
            let applet_type = applet_config
                .map(|c| c.extends.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(name);
            if applet_type != "notifications" {
                continue;
            }

            let popup_config: NotificationsConfig = applet_config
                .map(|c| c.settings.clone().try_into().unwrap_or_default())
                .unwrap_or_default();
            return popup_config.show_popup.then_some(popup_config);
        }
    }

    None
}

fn panel_applet_names(panel: &PanelConfig) -> impl Iterator<Item = &String> {
    panel
        .left
        .iter()
        .chain(panel.center.iter())
        .chain(panel.right.iter())
}

fn popup_sync_plan(
    old: Option<NotificationsConfig>,
    new: Option<NotificationsConfig>,
) -> PopupSyncPlan {
    match (old, new) {
        (None, None) => PopupSyncPlan::Keep,
        (None, Some(config)) => PopupSyncPlan::Create(config),
        (Some(_), None) => PopupSyncPlan::Remove,
        (Some(old), Some(new)) => {
            if old == new {
                PopupSyncPlan::Keep
            } else {
                PopupSyncPlan::Update(new)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::config::{AppletConfig, Config, PanelPosition};
    use toml::Value;

    fn panel(left: &[&str], center: &[&str], right: &[&str]) -> PanelConfig {
        PanelConfig {
            position: PanelPosition::Top,
            height: 36,
            margin: Default::default(),
            left: left.iter().map(|name| name.to_string()).collect(),
            center: center.iter().map(|name| name.to_string()).collect(),
            right: right.iter().map(|name| name.to_string()).collect(),
        }
    }

    fn notifications_applet(settings: Value) -> AppletConfig {
        AppletConfig {
            extends: "notifications".to_string(),
            settings,
        }
    }

    #[test]
    fn popup_config_uses_first_notifications_applet_in_panel_order() {
        let mut config = Config::default();
        config.panels = vec![panel(&["clock"], &["notif-b"], &["notif-a"])];
        config.applets.insert(
            "notif-a".into(),
            notifications_applet(toml::from_str(r#"popup_position = "top-left""#).unwrap()),
        );
        config.applets.insert(
            "notif-b".into(),
            notifications_applet(toml::from_str(r#"popup_position = "bottom-right""#).unwrap()),
        );

        let popup = notifications_popup_config(&config).expect("popup config");
        assert_eq!(popup.popup_position, "bottom-right");
    }

    #[test]
    fn popup_config_returns_none_when_first_notifications_applet_disables_popup() {
        let mut config = Config::default();
        config.panels = vec![panel(&["notif-a"], &[], &["notif-b"])];
        config.applets.insert(
            "notif-a".into(),
            notifications_applet(toml::from_str(r#"show_popup = false"#).unwrap()),
        );
        config.applets.insert(
            "notif-b".into(),
            notifications_applet(toml::from_str(r#"show_popup = true"#).unwrap()),
        );

        assert!(notifications_popup_config(&config).is_none());
    }

    #[test]
    fn popup_sync_plan_updates_existing_popup_in_place() {
        let old: NotificationsConfig =
            toml::from_str(r#"popup_position = "top-left""#).expect("old popup config");
        let new: NotificationsConfig =
            toml::from_str(r#"popup_position = "bottom-right""#).expect("new popup config");

        assert_eq!(
            popup_sync_plan(Some(old), Some(new.clone())),
            PopupSyncPlan::Update(new)
        );
    }

    #[test]
    fn popup_sync_plan_creates_popup_when_enabled_later() {
        let new: NotificationsConfig =
            toml::from_str(r#"show_popup = true"#).expect("new popup config");

        assert_eq!(
            popup_sync_plan(None, Some(new.clone())),
            PopupSyncPlan::Create(new)
        );
    }

    #[test]
    fn popup_sync_plan_removes_popup_when_disabled() {
        let old: NotificationsConfig =
            toml::from_str(r#"show_popup = true"#).expect("old popup config");

        assert_eq!(popup_sync_plan(Some(old), None), PopupSyncPlan::Remove);
    }
}
