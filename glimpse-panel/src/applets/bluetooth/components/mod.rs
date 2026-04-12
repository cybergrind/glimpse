mod device_list;
mod device_row;
mod hero;

pub use device_list::{BluetoothDeviceList, BluetoothDeviceListInput, BluetoothDeviceListOutput};
pub use device_row::{BluetoothDeviceRow, BluetoothDeviceRowInput, BluetoothDeviceRowOutput};
pub use hero::{BluetoothHero, BluetoothHeroInput, BluetoothHeroOutput};

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
    Trust(bool),
    Forget,
}
