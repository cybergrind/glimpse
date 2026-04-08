pub mod device_list;
pub mod device_row;
pub mod hero;

use std::rc::Rc;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BtDevice {
    pub address: String,
    pub name: String,
    pub icon: String,
    pub device_type: String,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    pub battery: Option<u8>,
    pub rssi: Option<i16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothDeviceAction {
    Connect,
    Disconnect,
    Pair,
    Forget,
}

#[derive(Debug, Clone)]
pub enum BluetoothCommand {
    SetPowered(bool),
    DeviceAction {
        address: String,
        name: String,
        action: BluetoothDeviceAction,
    },
}

pub type BluetoothCommandSender = Rc<dyn Fn(BluetoothCommand)>;
