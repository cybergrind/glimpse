use glimpse::config::AppletConfig;

use crate::services::ServicesHandle;
use relm4::{Component, ComponentController};

use super::{
    audio, battery, bluetooth, brightness, clock, exec, keyboard, mpris, network, notifications,
    pager, power, privacy, session, tray, weather, AppletController,
};

pub type AppletCreateFn = for<'a> fn(AppletCreateRequest<'a>) -> Option<AppletController>;
pub type AppletReconfigureFn =
    for<'a> fn(&AppletController, AppletReconfigureRequest<'a>) -> ReconfigureOutcome;

#[derive(Clone)]
pub struct AppletCreateRequest<'a> {
    pub applet_config: Option<&'a AppletConfig>,
    pub name: &'a str,
    pub system: zbus::Connection,
    pub services: ServicesHandle,
}

impl<'a> AppletCreateRequest<'a> {
    pub fn applet_type(&self) -> &'a str {
        resolved_applet_type(self.applet_config, self.name)
    }
}

#[derive(Debug, Clone)]
pub struct AppletReconfigureRequest<'a> {
    pub applet_config: Option<&'a AppletConfig>,
    pub name: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconfigureOutcome {
    Updated,
    RecreateRequired,
    Unsupported,
}

impl ReconfigureOutcome {
    pub fn needs_recreate(self) -> bool {
        matches!(self, Self::RecreateRequired | Self::Unsupported)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AppletSpec {
    pub applet_type: &'static str,
    pub create: AppletCreateFn,
    pub reconfigure: Option<AppletReconfigureFn>,
}

impl AppletSpec {
    pub fn create(self, request: AppletCreateRequest<'_>) -> Option<AppletController> {
        (self.create)(request)
    }

    pub fn reconfigure(
        self,
        controller: &AppletController,
        request: AppletReconfigureRequest<'_>,
    ) -> ReconfigureOutcome {
        self.reconfigure
            .map(|reconfigure| reconfigure(controller, request))
            .unwrap_or(ReconfigureOutcome::Unsupported)
    }
}

pub fn resolved_applet_type<'a>(applet_config: Option<&'a AppletConfig>, name: &'a str) -> &'a str {
    if let Some(applet_config) = applet_config {
        if !applet_config.extends.is_empty() {
            return applet_config.extends.as_str();
        }
    }

    name
}

pub fn spec_for(applet_type: &str) -> Option<&'static AppletSpec> {
    registry().iter().find(|spec| spec.applet_type == applet_type)
}

pub fn registry() -> &'static [AppletSpec] {
    &APPLET_SPECS
}

pub fn create_applet(
    applet_config: Option<&AppletConfig>,
    name: &str,
    _dbus: zbus::Connection,
    system: zbus::Connection,
    services: ServicesHandle,
) -> Option<AppletController> {
    let request = AppletCreateRequest {
        applet_config,
        name,
        system,
        services,
    };
    let applet_type = request.applet_type();
    tracing::debug!(name, applet_type, "creating applet");
    let spec = spec_for(applet_type)?;
    spec.create(request)
}

pub fn reconfigure_applet(
    controller: &AppletController,
    request: AppletReconfigureRequest<'_>,
) -> ReconfigureOutcome {
    let applet_type = resolved_applet_type(request.applet_config, request.name);
    let Some(spec) = spec_for(applet_type) else {
        return ReconfigureOutcome::RecreateRequired;
    };

    if controller_matches_type(controller, applet_type) {
        spec.reconfigure(controller, request)
    } else {
        ReconfigureOutcome::RecreateRequired
    }
}

fn controller_matches_type(controller: &AppletController, applet_type: &str) -> bool {
    matches!(
        (controller, applet_type),
        (AppletController::Audio(_), "audio")
            | (AppletController::Battery(_), "battery")
            | (AppletController::Bluetooth(_), "bluetooth")
            | (AppletController::Brightness(_), "brightness")
            | (AppletController::Clock(_), "clock")
            | (AppletController::Exec(_), "exec")
            | (AppletController::Keyboard(_), "keyboard")
            | (AppletController::Mpris(_), "mpris")
            | (AppletController::Network(_), "network")
            | (AppletController::Notifications(_), "notifications")
            | (AppletController::Pager(_), "pager")
            | (AppletController::Power(_), "power")
            | (AppletController::Privacy(_), "privacy")
            | (AppletController::Session(_), "session")
            | (AppletController::Tray(_), "tray")
            | (AppletController::Weather(_), "weather")
    )
}

const APPLET_SPECS: &[AppletSpec] = &[
    AppletSpec {
        applet_type: "audio",
        create: create_audio,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "bluetooth",
        create: create_bluetooth,
        reconfigure: Some(reconfigure_bluetooth),
    },
    AppletSpec {
        applet_type: "brightness",
        create: create_brightness,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "network",
        create: create_network,
        reconfigure: Some(reconfigure_network),
    },
    AppletSpec {
        applet_type: "mpris",
        create: create_mpris,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "exec",
        create: create_exec,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "notifications",
        create: create_notifications,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "battery",
        create: create_battery,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "clock",
        create: create_clock,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "power",
        create: create_power,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "privacy",
        create: create_privacy,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "tray",
        create: create_tray,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "weather",
        create: create_weather,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "session",
        create: create_session,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "keyboard",
        create: create_keyboard,
        reconfigure: None,
    },
    AppletSpec {
        applet_type: "pager",
        create: create_pager,
        reconfigure: None,
    },
];

fn create_audio(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: audio::AudioConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = audio::Audio::builder()
        .launch(audio::AudioInit { config })
        .detach();
    Some(AppletController::Audio(applet))
}

fn create_bluetooth(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: bluetooth::BluetoothConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = bluetooth::Bluetooth::builder()
        .launch(bluetooth::BluetoothInit {
            config,
            service: request.services.bluetooth.clone(),
        })
        .detach();
    Some(AppletController::Bluetooth(applet))
}

fn reconfigure_bluetooth(
    controller: &AppletController,
    request: AppletReconfigureRequest<'_>,
) -> ReconfigureOutcome {
    let AppletController::Bluetooth(controller) = controller else {
        return ReconfigureOutcome::RecreateRequired;
    };
    let config: bluetooth::BluetoothConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    controller.emit(bluetooth::BluetoothMsg::Reconfigure(config));
    ReconfigureOutcome::Updated
}

fn create_brightness(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: brightness::BrightnessConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = brightness::Brightness::builder()
        .launch(brightness::BrightnessInit {
            config,
            service: request.services.brightness.clone(),
        })
        .detach();
    Some(AppletController::Brightness(applet))
}

fn create_network(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: network::NetworkConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = network::Network::builder()
        .launch(network::NetworkInit {
            config,
            service: request.services.network.clone(),
        })
        .detach();
    Some(AppletController::Network(applet))
}

fn reconfigure_network(
    controller: &AppletController,
    request: AppletReconfigureRequest<'_>,
) -> ReconfigureOutcome {
    let AppletController::Network(controller) = controller else {
        return ReconfigureOutcome::RecreateRequired;
    };
    let config: network::NetworkConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    controller.emit(network::NetworkMsg::Reconfigure(config));
    ReconfigureOutcome::Updated
}

fn create_mpris(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: mpris::MprisConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = mpris::Mpris::builder()
        .launch(mpris::MprisInit {
            config,
            service: request.services.mpris.clone(),
        })
        .detach();
    Some(AppletController::Mpris(applet))
}

fn create_exec(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: exec::ExecConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    if config.command.is_empty() {
        tracing::error!(name = request.name, "exec applet requires a non-empty command");
        return None;
    }
    let applet = exec::Exec::builder()
        .launch(exec::ExecInit {
            name: request.name.to_string(),
            config,
        })
        .detach();
    Some(AppletController::Exec(applet))
}

fn create_notifications(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: notifications::NotificationsConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = notifications::Notifications::builder()
        .launch(notifications::NotificationsInit {
            config,
            service: request.services.notifications.clone(),
        })
        .detach();
    Some(AppletController::Notifications(applet))
}

fn create_battery(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: battery::BatteryConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = battery::Battery::builder()
        .launch(battery::BatteryInit {
            config,
            conn: request.system.clone(),
        })
        .detach();
    Some(AppletController::Battery(applet))
}

fn create_clock(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: clock::ClockConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = clock::Clock::builder()
        .launch(clock::ClockInit {
            config,
            service: request.services.calendar.clone(),
        })
        .detach();
    Some(AppletController::Clock(applet))
}

fn create_power(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: power::PowerConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = power::Power::builder()
        .launch(power::PowerInit {
            config,
            dbus: request.system.clone(),
        })
        .detach();
    Some(AppletController::Power(applet))
}

fn create_privacy(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: privacy::PrivacyConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = privacy::Privacy::builder()
        .launch(privacy::PrivacyInit {
            config,
            service: request.services.privacy.clone(),
        })
        .detach();
    Some(AppletController::Privacy(applet))
}

fn create_tray(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: tray::TrayConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = tray::Tray::builder()
        .launch(tray::TrayInit {
            config,
            service: request.services.tray.clone(),
        })
        .detach();
    Some(AppletController::Tray(applet))
}

fn create_weather(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: weather::WeatherConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = weather::Weather::builder().launch(config).detach();
    Some(AppletController::Weather(applet))
}

fn create_session(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: session::SessionConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = session::Session::builder()
        .launch(session::SessionInit {
            config,
            conn: request.system.clone(),
        })
        .detach();
    Some(AppletController::Session(applet))
}

fn create_keyboard(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: keyboard::KeyboardConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = keyboard::Keyboard::builder()
        .launch(keyboard::KeyboardInit {
            config,
            service: request.services.keyboard_layout.clone(),
        })
        .detach();
    Some(AppletController::Keyboard(applet))
}

fn create_pager(request: AppletCreateRequest<'_>) -> Option<AppletController> {
    let config: pager::PagerConfig = request
        .applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = pager::Pager::builder()
        .launch(pager::PagerInit {
            config,
            service: request.services.workspace.clone(),
        })
        .detach();
    Some(AppletController::Pager(applet))
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::config::AppletConfig;

    #[test]
    fn resolved_applet_type_prefers_extends_when_present() {
        let applet_config: AppletConfig = toml::from_str(
            r#"
extends = "network"
"#,
        )
        .expect("applet config");

        assert_eq!(resolved_applet_type(Some(&applet_config), "clock"), "network");
    }

    #[test]
    fn resolved_applet_type_falls_back_to_name_for_empty_extends() {
        let applet_config: AppletConfig = toml::from_str(
            r#"
extends = ""
"#,
        )
        .expect("applet config");

        assert_eq!(resolved_applet_type(Some(&applet_config), "clock"), "clock");
    }

    #[test]
    fn registry_lists_current_builtin_types() {
        let types = registry().iter().map(|spec| spec.applet_type).collect::<Vec<_>>();

        assert_eq!(
            types,
            vec![
                "audio",
                "bluetooth",
                "brightness",
                "network",
                "mpris",
                "exec",
                "notifications",
                "battery",
                "clock",
                "power",
                "privacy",
                "tray",
                "weather",
                "session",
                "keyboard",
                "pager",
            ]
        );
    }

    #[test]
    fn known_spec_reports_no_reconfigure_yet() {
        let spec = spec_for("clock").expect("clock spec");

        assert_eq!(spec.applet_type, "clock");
        assert!(spec.reconfigure.is_none());
    }

    #[test]
    fn unknown_spec_is_missing() {
        assert!(spec_for("does-not-exist").is_none());
    }
}
