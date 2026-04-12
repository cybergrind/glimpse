use crate::brightness::provider::BrightnessSnapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrightnessServiceHealth {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrightnessActiveAdjustment {
    SetDisplayPercent {
        display_id: String,
        percent: u8,
    },
    AdjustDisplayPercent {
        display_id: String,
        delta_percent: i32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrightnessServiceState {
    pub health: BrightnessServiceHealth,
    pub snapshot: BrightnessSnapshot,
    pub active_adjustment: Option<BrightnessActiveAdjustment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrightnessServiceCommand {
    Refresh,
    PopoverOpened,
    PopoverClosed,
    SetDisplayPercent {
        display_id: String,
        percent: u8,
    },
    AdjustDisplayPercent {
        display_id: String,
        delta_percent: i32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brightness_service_protocol_roundtrip() {
        let state = BrightnessServiceState {
            health: BrightnessServiceHealth::Starting,
            snapshot: BrightnessSnapshot::default(),
            active_adjustment: Some(BrightnessActiveAdjustment::AdjustDisplayPercent {
                display_id: "backlight:intel".into(),
                delta_percent: 5,
            }),
        };

        let cloned = state.clone();
        let command = BrightnessServiceCommand::SetDisplayPercent {
            display_id: "ddc:1".into(),
            percent: 42,
        };

        assert_eq!(cloned, state);
        assert_eq!(
            command,
            BrightnessServiceCommand::SetDisplayPercent {
                display_id: "ddc:1".into(),
                percent: 42,
            }
        );
    }
}
