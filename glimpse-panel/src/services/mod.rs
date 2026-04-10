use glimpse::{bluetooth::BluetoothServiceHandle, network::NetworkServiceHandle};

#[derive(Clone)]
pub struct ServicesHandle {
    pub bluetooth: BluetoothServiceHandle,
    pub network: NetworkServiceHandle,
}

pub struct Services {
    pub handle: ServicesHandle,
}

impl Services {
    pub fn new(system: zbus::Connection) -> Self {
        let bluetooth = BluetoothServiceHandle::new(system.clone());
        let network = NetworkServiceHandle::new(system);
        Self {
            handle: ServicesHandle { bluetooth, network },
        }
    }
}
