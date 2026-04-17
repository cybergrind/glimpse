use adw::prelude::GtkWindowExt;
use relm4::{Component, ComponentController, Controller};

use glimpse::config::Config;

use crate::{
    panels,
    panels::diff::{PanelKey, build_panel_keys},
};
use glimpse::services::ServicesHandle;

pub(super) struct PanelState {
    pub(super) key: PanelKey,
    pub(super) controller: Controller<panels::Panel>,
}

pub(super) fn setup_panels(
    config: &Config,
    dbus: zbus::Connection,
    system: zbus::Connection,
    services: ServicesHandle,
) -> Vec<PanelState> {
    let mut panels = vec![];
    for (panel_key, panel_config) in build_panel_keys(&config.panels)
        .into_iter()
        .zip(config.panels.iter())
    {
        let panel_init = panels::Init {
            panel_key: panel_key.clone(),
            config: panel_config.clone(),
            applet_configs: config.applets.clone(),
            dbus: dbus.clone(),
            system: system.clone(),
            services: services.clone(),
        };
        let panel = panels::Panel::builder().launch(panel_init).detach();
        panels.push(PanelState {
            key: panel_key,
            controller: panel,
        });
    }
    panels
}

pub(super) fn reconfigure_panels(
    panels: &mut Vec<PanelState>,
    config: &Config,
    dbus: zbus::Connection,
    system: zbus::Connection,
    services: ServicesHandle,
) {
    let mut current = std::mem::take(panels)
        .into_iter()
        .map(|state| (state.key.clone(), state))
        .collect::<std::collections::HashMap<_, _>>();
    let mut next_panels = Vec::with_capacity(config.panels.len());

    for (panel_key, panel_config) in build_panel_keys(&config.panels)
        .into_iter()
        .zip(config.panels.iter())
    {
        if let Some(existing) = current.remove(&panel_key) {
            existing
                .controller
                .emit(panels::component::Input::Reconfigure(
                    panels::component::PanelRuntimeConfig {
                        panel_key: panel_key.clone(),
                        config: panel_config.clone(),
                        applet_configs: config.applets.clone(),
                        dbus: dbus.clone(),
                        system: system.clone(),
                        services: services.clone(),
                    },
                ));
            next_panels.push(existing);
            continue;
        }

        let panel = panels::Panel::builder()
            .launch(panels::Init {
                panel_key: panel_key.clone(),
                config: panel_config.clone(),
                applet_configs: config.applets.clone(),
                dbus: dbus.clone(),
                system: system.clone(),
                services: services.clone(),
            })
            .detach();
        next_panels.push(PanelState {
            key: panel_key,
            controller: panel,
        });
    }

    for state in current.into_values() {
        state.controller.widget().close();
    }

    *panels = next_panels;
}
