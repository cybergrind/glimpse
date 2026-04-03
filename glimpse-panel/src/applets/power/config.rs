use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PowerConfig {
    pub percentage: bool,
    pub low_battery_treshold: u8,
    pub hide_on_no_battery: bool,
    pub format: String,
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            percentage: false,
            low_battery_treshold: 15,
            hide_on_no_battery: true,
            format: String::from("{}%"),
        }
    }
}
