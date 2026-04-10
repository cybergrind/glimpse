use glimpse::{
    bluetooth::BluetoothServiceHandle, network::NetworkServiceHandle, tray::TrayServiceHandle,
};

#[derive(Clone)]
pub struct ServicesHandle {
    pub bluetooth: BluetoothServiceHandle,
    pub network: NetworkServiceHandle,
    pub tray: TrayServiceHandle,
}

pub struct Services {
    pub handle: ServicesHandle,
}

impl Services {
    pub fn new(system: zbus::Connection) -> Self {
        let bluetooth = BluetoothServiceHandle::new(system.clone());
        let network = NetworkServiceHandle::new(system);
        let tray = TrayServiceHandle::new();
        Self {
            handle: ServicesHandle {
                bluetooth,
                network,
                tray,
            },
        }
    }
}
