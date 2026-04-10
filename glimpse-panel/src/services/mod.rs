use glimpse::{
    bluetooth::BluetoothServiceHandle,
    calendar::CalendarServiceHandle,
    mpris::MprisServiceHandle,
    network::NetworkServiceHandle,
    notifications::NotificationsServiceHandle,
    privacy::PrivacyServiceHandle,
    tray::TrayServiceHandle,
};

#[derive(Clone)]
pub struct ServicesHandle {
    pub bluetooth: BluetoothServiceHandle,
    pub calendar: CalendarServiceHandle,
    pub mpris: MprisServiceHandle,
    pub network: NetworkServiceHandle,
    pub tray: TrayServiceHandle,
    pub notifications: NotificationsServiceHandle,
    pub privacy: PrivacyServiceHandle,
}

pub struct Services {
    pub handle: ServicesHandle,
}

impl Services {
    pub fn new(session: zbus::Connection, system: zbus::Connection) -> Self {
        let bluetooth = BluetoothServiceHandle::new(system.clone());
        let calendar = CalendarServiceHandle::new(session.clone());
        let mpris = MprisServiceHandle::new(session.clone());
        let network = NetworkServiceHandle::new(system.clone());
        let privacy = PrivacyServiceHandle::new(session.clone());
        let tray = TrayServiceHandle::new();
        let notifications = NotificationsServiceHandle::new(session);
        Self {
            handle: ServicesHandle {
                bluetooth,
                calendar,
                mpris,
                network,
                tray,
                notifications,
                privacy,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ServicesHandle;

    #[test]
    fn services_handle_exposes_notifications() {
        fn assert_notifications_field(handle: &ServicesHandle) {
            let _ = &handle.notifications;
        }

        let _ = assert_notifications_field;
    }
}
