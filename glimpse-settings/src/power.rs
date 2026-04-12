use glimpse::providers::{
    battery::{BatteryDevice, BatteryEvent, BatteryState, BatteryStatus},
    power::{PowerEvent, PowerProfiles},
    power_policy::{PowerPolicyAction, PowerPolicyEvent, PowerPolicySnapshot},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalPowerUpdate {
    Unchanged,
    SyncedClean,
    BaselineUpdated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerApplyPlan {
    pub apply_profile: bool,
    pub apply_policy: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PowerDraft {
    pub profile: String,
    pub policy: PowerPolicySnapshot,
}

#[derive(Debug, Clone, Default)]
pub struct PowerPageState {
    pub battery_status: BatteryStatus,
    pub devices: Vec<BatteryDevice>,
    pub profiles: PowerProfiles,
    pub baseline: PowerDraft,
    pub draft: PowerDraft,
}

impl PowerPageState {
    pub fn apply_battery_event(&mut self, event: &BatteryEvent) -> bool {
        match event {
            BatteryEvent::StatusChanged(status) => {
                if self.battery_status.percentage == status.percentage
                    && std::mem::discriminant(&self.battery_status.state)
                        == std::mem::discriminant(&status.state)
                    && self.battery_status.time_to_empty == status.time_to_empty
                    && self.battery_status.time_to_full == status.time_to_full
                    && (self.battery_status.capacity - status.capacity).abs() < f64::EPSILON
                    && self.battery_status.charge_threshold == status.charge_threshold
                    && self.battery_status.icon_name == status.icon_name
                    && self.battery_status.on_battery == status.on_battery
                    && self.battery_status.present == status.present
                {
                    return false;
                }
                self.battery_status = status.clone();
                true
            }
            BatteryEvent::DevicesChanged(devices) => {
                let changed = self.devices.len() != devices.len()
                    || self
                        .devices
                        .iter()
                        .zip(devices.iter())
                        .any(|(left, right)| {
                            left.path != right.path
                                || left.model != right.model
                                || left.icon_name != right.icon_name
                                || (left.percentage - right.percentage).abs() > f64::EPSILON
                        });
                if changed {
                    self.devices = devices.clone();
                }
                changed
            }
        }
    }

    pub fn apply_power_event(&mut self, event: &PowerEvent) -> bool {
        match event {
            PowerEvent::ProfilesChanged(profiles) => {
                let changed = self.profiles.active != profiles.active
                    || self.profiles.available != profiles.available
                    || self.profiles.performance_degraded != profiles.performance_degraded;
                if !changed {
                    return false;
                }

                self.profiles = profiles.clone();
                if self.is_dirty() {
                    self.baseline.profile = profiles.active.clone();
                } else {
                    self.baseline.profile = profiles.active.clone();
                    self.draft.profile = profiles.active.clone();
                }
                true
            }
            PowerEvent::ActionsChanged(_) => false,
        }
    }

    pub fn apply_policy_event(&mut self, event: &PowerPolicyEvent) -> ExternalPowerUpdate {
        match event {
            PowerPolicyEvent::Changed(snapshot) => {
                if self.baseline.policy == *snapshot {
                    if self.draft.policy == *snapshot {
                        return ExternalPowerUpdate::Unchanged;
                    }
                    return ExternalPowerUpdate::BaselineUpdated;
                }

                if self.is_dirty() {
                    self.baseline.policy = snapshot.clone();
                    ExternalPowerUpdate::BaselineUpdated
                } else {
                    self.baseline.policy = snapshot.clone();
                    self.draft.policy = snapshot.clone();
                    ExternalPowerUpdate::SyncedClean
                }
            }
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.draft != self.baseline
    }

    pub fn set_profile(&mut self, profile: &str) {
        self.draft.profile = profile.to_string();
    }

    pub fn set_policy(&mut self, policy: PowerPolicySnapshot) {
        self.draft.policy = policy;
    }

    pub fn reset_draft(&mut self) {
        self.draft = self.baseline.clone();
    }

    pub fn apply_plan(&self) -> PowerApplyPlan {
        PowerApplyPlan {
            apply_profile: self.draft.profile != self.baseline.profile,
            apply_policy: self.draft.policy != self.baseline.policy,
        }
    }
}

pub fn format_battery_summary(_status: &BatteryStatus) -> String {
    if !_status.present {
        return "No battery detected".into();
    }

    let state = match _status.state {
        BatteryState::Charging => "Charging",
        BatteryState::Discharging => "Discharging",
        BatteryState::Empty => "Empty",
        BatteryState::FullyCharged => "Fully Charged",
        BatteryState::PendingCharge => "Pending Charge",
        BatteryState::PendingDischarge => "Pending Discharge",
        BatteryState::Unknown => "Unknown",
    };

    let time = match _status.state {
        BatteryState::Discharging if _status.time_to_empty > 0 => {
            format!("About {} remaining", format_duration(_status.time_to_empty))
        }
        BatteryState::Charging if _status.time_to_full > 0 => {
            format!("About {} until full", format_duration(_status.time_to_full))
        }
        BatteryState::FullyCharged => "Fully charged".into(),
        _ => String::new(),
    };

    if time.is_empty() {
        format!("{state} • {}%", _status.percentage)
    } else {
        format!("{state} • {}% • {time}", _status.percentage)
    }
}

pub fn format_battery_health(_status: &BatteryStatus) -> String {
    if !_status.present {
        return "Unavailable".into();
    }

    let capacity = format!("{:.0}% capacity", _status.capacity);
    if _status.charge_threshold > 0 {
        format!(
            "{capacity} • Charge threshold {}%",
            _status.charge_threshold
        )
    } else {
        capacity
    }
}

pub fn action_label(_action: &PowerPolicyAction) -> String {
    match _action {
        PowerPolicyAction::Blank => "Blank Screen".into(),
        PowerPolicyAction::Suspend => "Suspend".into(),
        PowerPolicyAction::Hibernate => "Hibernate".into(),
        PowerPolicyAction::Shutdown => "Power Off".into(),
        PowerPolicyAction::Interactive => "Ask".into(),
        PowerPolicyAction::Nothing => "Do Nothing".into(),
        PowerPolicyAction::Logout => "Log Out".into(),
        PowerPolicyAction::Other(value) => value.clone(),
    }
}

pub fn action_from_label(_label: &str) -> PowerPolicyAction {
    match _label {
        "Blank Screen" => PowerPolicyAction::Blank,
        "Suspend" => PowerPolicyAction::Suspend,
        "Hibernate" => PowerPolicyAction::Hibernate,
        "Power Off" => PowerPolicyAction::Shutdown,
        "Ask" => PowerPolicyAction::Interactive,
        "Do Nothing" => PowerPolicyAction::Nothing,
        "Log Out" => PowerPolicyAction::Logout,
        other => PowerPolicyAction::Other(other.to_string()),
    }
}

pub fn action_options(current: &PowerPolicyAction) -> Vec<PowerPolicyAction> {
    let mut options = vec![
        PowerPolicyAction::Suspend,
        PowerPolicyAction::Hibernate,
        PowerPolicyAction::Shutdown,
        PowerPolicyAction::Logout,
        PowerPolicyAction::Nothing,
        PowerPolicyAction::Blank,
        PowerPolicyAction::Interactive,
    ];

    if matches!(current, PowerPolicyAction::Other(_)) && !options.contains(current) {
        options.push(current.clone());
    }

    options
}

pub fn profile_options(profiles: &PowerProfiles) -> Vec<String> {
    let mut options = Vec::new();

    if !profiles.active.is_empty() {
        options.push(profiles.active.clone());
    }

    for profile in &profiles.available {
        if !options.contains(profile) {
            options.push(profile.clone());
        }
    }

    options.sort_by(|left, right| profile_sort_key(left).cmp(&profile_sort_key(right)));

    options
}

fn profile_sort_key(profile: &str) -> (u8, String) {
    match profile {
        "power-saver" => (0, String::new()),
        "balanced" => (1, String::new()),
        "performance" => (2, String::new()),
        other => (3, other.to_ascii_lowercase()),
    }
}

pub fn seconds_to_minutes(seconds: u32) -> f64 {
    seconds as f64 / 60.0
}

pub fn minutes_to_seconds(minutes: f64) -> u32 {
    (minutes * 60.0).round().max(0.0) as u32
}

fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExternalPowerUpdate, PowerApplyPlan, PowerDraft, PowerPageState, action_from_label,
        action_label, action_options, format_battery_health, format_battery_summary,
        minutes_to_seconds, profile_options, seconds_to_minutes,
    };
    use glimpse::providers::{
        battery::{BatteryDevice, BatteryEvent, BatteryState, BatteryStatus, DeviceType},
        power::{PowerEvent, PowerProfiles},
        power_policy::{PowerPolicyAction, PowerPolicyEvent, PowerPolicySnapshot},
    };

    #[test]
    fn battery_event_updates_status_and_devices() {
        let mut state = PowerPageState::default();
        let status = BatteryStatus {
            present: true,
            percentage: 74,
            state: BatteryState::Discharging,
            time_to_empty: 18_300,
            capacity: 96.0,
            charge_threshold: 80,
            ..BatteryStatus::default()
        };
        let devices = vec![BatteryDevice {
            path: "/battery".into(),
            device_type: DeviceType::Battery,
            model: "Internal battery".into(),
            percentage: 74.0,
            state: BatteryState::Discharging,
            icon_name: "battery-good-symbolic".into(),
        }];

        assert!(state.apply_battery_event(&BatteryEvent::StatusChanged(status.clone())));
        assert!(state.apply_battery_event(&BatteryEvent::DevicesChanged(devices.clone())));
        assert_eq!(state.battery_status.percentage, 74);
        assert_eq!(state.devices.len(), 1);
        assert_eq!(state.devices[0].path, "/battery");
        assert_eq!(state.devices[0].model, "Internal battery");
    }

    #[test]
    fn power_profiles_event_updates_clean_draft_and_baseline() {
        let mut state = PowerPageState::default();
        let profiles = PowerProfiles {
            active: "balanced".into(),
            available: vec!["balanced".into(), "power-saver".into()],
            performance_degraded: String::new(),
        };

        assert!(state.apply_power_event(&PowerEvent::ProfilesChanged(profiles.clone())));
        assert_eq!(state.profiles.active, "balanced");
        assert_eq!(state.baseline.profile, "balanced");
        assert_eq!(state.draft.profile, "balanced");
    }

    #[test]
    fn power_profiles_event_only_updates_baseline_when_draft_is_dirty() {
        let mut state = PowerPageState {
            baseline: PowerDraft {
                profile: "balanced".into(),
                policy: PowerPolicySnapshot::default(),
            },
            draft: PowerDraft {
                profile: "power-saver".into(),
                policy: PowerPolicySnapshot::default(),
            },
            ..PowerPageState::default()
        };
        let profiles = PowerProfiles {
            active: "performance".into(),
            available: vec![
                "balanced".into(),
                "power-saver".into(),
                "performance".into(),
            ],
            performance_degraded: String::new(),
        };

        assert!(state.apply_power_event(&PowerEvent::ProfilesChanged(profiles)));
        assert_eq!(state.baseline.profile, "performance");
        assert_eq!(state.draft.profile, "power-saver");
    }

    #[test]
    fn policy_event_reconciles_like_other_settings_pages() {
        let mut state = PowerPageState {
            baseline: PowerDraft {
                profile: "balanced".into(),
                policy: PowerPolicySnapshot {
                    idle_delay: 300,
                    ..PowerPolicySnapshot::default()
                },
            },
            draft: PowerDraft {
                profile: "balanced".into(),
                policy: PowerPolicySnapshot {
                    idle_delay: 600,
                    ..PowerPolicySnapshot::default()
                },
            },
            ..PowerPageState::default()
        };
        let external = PowerPolicySnapshot {
            idle_delay: 120,
            ..PowerPolicySnapshot::default()
        };

        assert_eq!(
            state.apply_policy_event(&PowerPolicyEvent::Changed(external.clone())),
            ExternalPowerUpdate::BaselineUpdated
        );
        assert_eq!(state.baseline.policy, external);
        assert_eq!(state.draft.policy.idle_delay, 600);
    }

    #[test]
    fn battery_summary_and_health_are_human_readable() {
        let status = BatteryStatus {
            present: true,
            percentage: 74,
            state: BatteryState::Discharging,
            time_to_empty: 18_300,
            capacity: 96.0,
            charge_threshold: 80,
            ..BatteryStatus::default()
        };

        assert_eq!(
            format_battery_summary(&status),
            "Discharging • 74% • About 5h 05m remaining"
        );
        assert_eq!(
            format_battery_health(&status),
            "96% capacity • Charge threshold 80%"
        );
    }

    #[test]
    fn battery_health_is_unavailable_without_a_battery() {
        let status = BatteryStatus::default();

        assert_eq!(format_battery_summary(&status), "No battery detected");
        assert_eq!(format_battery_health(&status), "Unavailable");
    }

    #[test]
    fn policy_action_labels_round_trip() {
        let action = PowerPolicyAction::Hibernate;
        let label = action_label(&action);

        assert_eq!(label, "Hibernate");
        assert_eq!(action_from_label(&label), action);
    }

    #[test]
    fn action_options_preserve_unknown_current_value() {
        let current = PowerPolicyAction::Other("custom-action".into());

        let options = action_options(&current);

        assert!(options.contains(&current));
    }

    #[test]
    fn profile_options_preserve_active_profile_when_missing_from_available() {
        let profiles = PowerProfiles {
            active: "balanced".into(),
            available: vec!["power-saver".into(), "performance".into()],
            performance_degraded: String::new(),
        };

        let options = profile_options(&profiles);

        assert_eq!(
            options,
            vec![
                "power-saver".to_string(),
                "balanced".to_string(),
                "performance".to_string(),
            ]
        );
        assert!(options.contains(&"power-saver".into()));
        assert!(options.contains(&"performance".into()));
    }

    #[test]
    fn profile_options_keep_standard_profiles_in_stable_order() {
        let profiles = PowerProfiles {
            active: "performance".into(),
            available: vec![
                "balanced".into(),
                "performance".into(),
                "power-saver".into(),
            ],
            performance_degraded: String::new(),
        };

        let options = profile_options(&profiles);

        assert_eq!(
            options,
            vec![
                "power-saver".to_string(),
                "balanced".to_string(),
                "performance".to_string(),
            ]
        );
    }

    #[test]
    fn power_time_values_round_trip_through_minutes() {
        assert_eq!(seconds_to_minutes(0), 0.0);
        assert_eq!(seconds_to_minutes(90), 1.5);
        assert_eq!(minutes_to_seconds(0.0), 0);
        assert_eq!(minutes_to_seconds(1.5), 90);
    }

    #[test]
    fn apply_plan_skips_policy_when_only_profile_changed() {
        let state = PowerPageState {
            baseline: PowerDraft {
                profile: "balanced".into(),
                policy: PowerPolicySnapshot::default(),
            },
            draft: PowerDraft {
                profile: "power-saver".into(),
                policy: PowerPolicySnapshot::default(),
            },
            ..PowerPageState::default()
        };

        assert_eq!(
            state.apply_plan(),
            PowerApplyPlan {
                apply_profile: true,
                apply_policy: false,
            }
        );
    }

    #[test]
    fn apply_plan_skips_profile_when_only_policy_changed() {
        let state = PowerPageState {
            baseline: PowerDraft {
                profile: "balanced".into(),
                policy: PowerPolicySnapshot::default(),
            },
            draft: PowerDraft {
                profile: "balanced".into(),
                policy: PowerPolicySnapshot {
                    idle_delay: 60,
                    ..PowerPolicySnapshot::default()
                },
            },
            ..PowerPageState::default()
        };

        assert_eq!(
            state.apply_plan(),
            PowerApplyPlan {
                apply_profile: false,
                apply_policy: true,
            }
        );
    }
}
