pub mod control;
pub mod framework;
pub mod location;
pub mod runtime;

use crate::{
    bluetooth::BluetoothServiceHandle,
    brightness::BrightnessServiceHandle,
    calendar::CalendarServiceHandle,
    compositor::{KeyboardLayoutServiceHandle, WorkspaceServiceHandle},
    mpris::MprisServiceHandle,
    network::NetworkServiceHandle,
    night_light::{NightLightConfig, NightLightServiceHandle},
    notifications::NotificationsServiceHandle,
    privacy::PrivacyServiceHandle,
    tray::TrayServiceHandle,
};

#[derive(Clone)]
pub struct ServicesHandle {
    pub bluetooth: BluetoothServiceHandle,
    pub brightness: BrightnessServiceHandle,
    pub calendar: CalendarServiceHandle,
    pub mpris: MprisServiceHandle,
    pub network: NetworkServiceHandle,
    pub tray: TrayServiceHandle,
    pub notifications: NotificationsServiceHandle,
    pub privacy: PrivacyServiceHandle,
    pub workspace: WorkspaceServiceHandle,
    pub keyboard_layout: KeyboardLayoutServiceHandle,
    pub night_light: NightLightServiceHandle,
}

pub struct Services {
    pub handle: ServicesHandle,
}

impl Services {
    pub fn new(
        session: zbus::Connection,
        system: zbus::Connection,
        night_light: NightLightConfig,
    ) -> Self {
        let bluetooth = BluetoothServiceHandle::new(system.clone());
        let brightness = BrightnessServiceHandle::new(system.clone());
        let calendar = CalendarServiceHandle::new(session.clone());
        let mpris = MprisServiceHandle::new(session.clone());
        let network = NetworkServiceHandle::new(system.clone());
        let privacy = PrivacyServiceHandle::new(session.clone());
        let tray = TrayServiceHandle::new();
        let notifications = NotificationsServiceHandle::new(session);
        let workspace = WorkspaceServiceHandle::new();
        let keyboard_layout = KeyboardLayoutServiceHandle::new();
        let night_light = NightLightServiceHandle::new(night_light);
        Self {
            handle: ServicesHandle {
                bluetooth,
                brightness,
                calendar,
                mpris,
                network,
                tray,
                notifications,
                privacy,
                workspace,
                keyboard_layout,
                night_light,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ServicesHandle;

    #[test]
    fn services_handle_exposes_notifications() {
        fn assert_notifications_and_night_light_fields(handle: &ServicesHandle) {
            let _ = &handle.notifications;
            let _ = &handle.night_light;
        }

        let _ = assert_notifications_and_night_light_fields;
    }
}
