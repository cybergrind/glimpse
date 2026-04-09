pub mod bluetooth;

#[derive(Clone)]
pub struct ServicesHandle {
    pub bluetooth: bluetooth::BluetoothServiceHandle,
}

pub struct Services {
    pub handle: ServicesHandle,
}

impl Services {
    pub fn new(system: zbus::Connection) -> Self {
        let bluetooth = bluetooth::BluetoothServiceHandle::new_placeholder(system);
        Self {
            handle: ServicesHandle { bluetooth },
        }
    }
}
