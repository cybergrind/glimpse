#[derive(Clone)]
pub struct BluetoothServiceHandle;

impl BluetoothServiceHandle {
    pub fn new_placeholder(_system: zbus::Connection) -> Self {
        Self
    }
}
