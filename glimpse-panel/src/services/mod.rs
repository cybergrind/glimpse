use glimpse::{
    bluetooth::BluetoothServiceHandle,
    network::NetworkServiceHandle,
    notifications::NotificationsServiceHandle,
    tray::TrayServiceHandle,
};

#[derive(Clone)]
pub struct ServicesHandle {
    pub bluetooth: BluetoothServiceHandle,
    pub network: NetworkServiceHandle,
    pub tray: TrayServiceHandle,
    pub notifications: NotificationsServiceHandle,
}

pub struct Services {
    pub handle: ServicesHandle,
}

impl Services {
    pub fn new(session: zbus::Connection, system: zbus::Connection) -> Self {
        let bluetooth = BluetoothServiceHandle::new(system.clone());
        let network = NetworkServiceHandle::new(system.clone());
        let tray = TrayServiceHandle::new();
        let notifications = NotificationsServiceHandle::new(session);
        Self {
            handle: ServicesHandle {
                bluetooth,
                network,
                tray,
                notifications,
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
