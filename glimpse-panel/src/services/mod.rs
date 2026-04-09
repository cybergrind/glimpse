use glimpse::bluetooth::BluetoothServiceHandle;

#[derive(Clone)]
pub struct ServicesHandle {
    pub bluetooth: BluetoothServiceHandle,
}

pub struct Services {
    pub handle: ServicesHandle,
}

impl Services {
    pub fn new(system: zbus::Connection) -> Self {
        let bluetooth = BluetoothServiceHandle::new(system);
        Self {
            handle: ServicesHandle { bluetooth },
        }
    }
}
