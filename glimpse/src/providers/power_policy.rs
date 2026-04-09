use std::{borrow::Cow, time::Duration};

use gio::{Settings, SettingsSchema, SettingsSchemaSource, prelude::*};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const POWER_SCHEMA: &str = "org.gnome.settings-daemon.plugins.power";
const SESSION_SCHEMA: &str = "org.gnome.desktop.session";
const SCREENSAVER_SCHEMA: &str = "org.gnome.desktop.screensaver";
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(2);

const KEY_SLEEP_INACTIVE_BATTERY_TIMEOUT: &str = "sleep-inactive-battery-timeout";
const KEY_SLEEP_INACTIVE_BATTERY_TYPE: &str = "sleep-inactive-battery-type";
const KEY_SLEEP_INACTIVE_AC_TIMEOUT: &str = "sleep-inactive-ac-timeout";
const KEY_SLEEP_INACTIVE_AC_TYPE: &str = "sleep-inactive-ac-type";
const KEY_POWER_SAVER_PROFILE_ON_LOW_BATTERY: &str = "power-saver-profile-on-low-battery";
const KEY_IDLE_DELAY: &str = "idle-delay";
const KEY_IDLE_ACTIVATION_ENABLED: &str = "idle-activation-enabled";
const KEY_LOCK_ENABLED: &str = "lock-enabled";
const KEY_LOCK_DELAY: &str = "lock-delay";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PowerPolicyAction {
    Blank,
    Suspend,
    Hibernate,
    Shutdown,
    Interactive,
    Nothing,
    Logout,
    Other(String),
}

impl Default for PowerPolicyAction {
    fn default() -> Self {
        Self::Nothing
    }
}

impl PowerPolicyAction {
    pub fn from_gsettings(value: &str) -> Self {
        match value {
            "blank" => Self::Blank,
            "suspend" => Self::Suspend,
            "hibernate" => Self::Hibernate,
            "shutdown" => Self::Shutdown,
            "interactive" => Self::Interactive,
            "nothing" => Self::Nothing,
            "logout" => Self::Logout,
            other => Self::Other(other.to_owned()),
        }
    }

    pub fn as_gsettings(&self) -> Cow<'_, str> {
        match self {
            Self::Blank => Cow::Borrowed("blank"),
            Self::Suspend => Cow::Borrowed("suspend"),
            Self::Hibernate => Cow::Borrowed("hibernate"),
            Self::Shutdown => Cow::Borrowed("shutdown"),
            Self::Interactive => Cow::Borrowed("interactive"),
            Self::Nothing => Cow::Borrowed("nothing"),
            Self::Logout => Cow::Borrowed("logout"),
            Self::Other(value) => Cow::Borrowed(value.as_str()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct PowerPolicyCapabilities {
    pub sleep_inactive_battery_timeout: bool,
    pub sleep_inactive_battery_action: bool,
    pub sleep_inactive_ac_timeout: bool,
    pub sleep_inactive_ac_action: bool,
    pub power_saver_profile_on_low_battery: bool,
    pub idle_delay: bool,
    pub idle_activation_enabled: bool,
    pub lock_enabled: bool,
    pub lock_delay: bool,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct PowerPolicySnapshot {
    pub capabilities: PowerPolicyCapabilities,
    pub sleep_inactive_battery_timeout: u32,
    pub sleep_inactive_battery_action: PowerPolicyAction,
    pub sleep_inactive_ac_timeout: u32,
    pub sleep_inactive_ac_action: PowerPolicyAction,
    pub power_saver_profile_on_low_battery: bool,
    pub idle_delay: u32,
    pub idle_activation_enabled: bool,
    pub lock_enabled: bool,
    pub lock_delay: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PowerPolicyEvent {
    Changed(PowerPolicySnapshot),
}

pub struct PowerPolicySettings {
    capabilities: PowerPolicyCapabilities,
    power: Option<SettingsBinding>,
    session: Option<SettingsBinding>,
    screensaver: Option<SettingsBinding>,
    poll_interval: Duration,
}

impl PowerPolicySettings {
    pub fn new() -> Self {
        Self::from_schema_source(SettingsSchemaSource::default(), DEFAULT_POLL_INTERVAL)
    }

    pub fn capabilities(&self) -> &PowerPolicyCapabilities {
        &self.capabilities
    }

    pub fn load(&self) -> anyhow::Result<PowerPolicySnapshot> {
        let mut snapshot = PowerPolicySnapshot {
            capabilities: self.capabilities.clone(),
            ..PowerPolicySnapshot::default()
        };

        if let Some(power) = &self.power {
            if snapshot.capabilities.sleep_inactive_battery_timeout {
                snapshot.sleep_inactive_battery_timeout =
                    power.settings.uint(KEY_SLEEP_INACTIVE_BATTERY_TIMEOUT);
            }

            if snapshot.capabilities.sleep_inactive_battery_action {
                snapshot.sleep_inactive_battery_action = PowerPolicyAction::from_gsettings(
                    power.settings.string(KEY_SLEEP_INACTIVE_BATTERY_TYPE).as_str(),
                );
            }

            if snapshot.capabilities.sleep_inactive_ac_timeout {
                snapshot.sleep_inactive_ac_timeout = power.settings.uint(KEY_SLEEP_INACTIVE_AC_TIMEOUT);
            }

            if snapshot.capabilities.sleep_inactive_ac_action {
                snapshot.sleep_inactive_ac_action = PowerPolicyAction::from_gsettings(
                    power.settings.string(KEY_SLEEP_INACTIVE_AC_TYPE).as_str(),
                );
            }

            if snapshot.capabilities.power_saver_profile_on_low_battery {
                snapshot.power_saver_profile_on_low_battery = power
                    .settings
                    .boolean(KEY_POWER_SAVER_PROFILE_ON_LOW_BATTERY);
            }
        }

        if let Some(session) = &self.session {
            if snapshot.capabilities.idle_delay {
                snapshot.idle_delay = session.settings.uint(KEY_IDLE_DELAY);
            }
        }

        if let Some(screensaver) = &self.screensaver {
            if snapshot.capabilities.idle_activation_enabled {
                snapshot.idle_activation_enabled =
                    screensaver.settings.boolean(KEY_IDLE_ACTIVATION_ENABLED);
            }

            if snapshot.capabilities.lock_enabled {
                snapshot.lock_enabled = screensaver.settings.boolean(KEY_LOCK_ENABLED);
            }

            if snapshot.capabilities.lock_delay {
                snapshot.lock_delay = screensaver.settings.uint(KEY_LOCK_DELAY);
            }
        }

        Ok(snapshot)
    }

    pub fn apply(&self, snapshot: &PowerPolicySnapshot) -> anyhow::Result<()> {
        if let Some(power) = &self.power {
            if self.capabilities.sleep_inactive_battery_timeout {
                power
                    .settings
                    .set_uint(
                        KEY_SLEEP_INACTIVE_BATTERY_TIMEOUT,
                        snapshot.sleep_inactive_battery_timeout,
                    )
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
            }

            if self.capabilities.sleep_inactive_battery_action {
                power
                    .settings
                    .set_string(
                        KEY_SLEEP_INACTIVE_BATTERY_TYPE,
                        snapshot.sleep_inactive_battery_action.as_gsettings().as_ref(),
                    )
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
            }

            if self.capabilities.sleep_inactive_ac_timeout {
                power
                    .settings
                    .set_uint(KEY_SLEEP_INACTIVE_AC_TIMEOUT, snapshot.sleep_inactive_ac_timeout)
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
            }

            if self.capabilities.sleep_inactive_ac_action {
                power
                    .settings
                    .set_string(
                        KEY_SLEEP_INACTIVE_AC_TYPE,
                        snapshot.sleep_inactive_ac_action.as_gsettings().as_ref(),
                    )
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
            }

            if self.capabilities.power_saver_profile_on_low_battery {
                power
                    .settings
                    .set_boolean(
                        KEY_POWER_SAVER_PROFILE_ON_LOW_BATTERY,
                        snapshot.power_saver_profile_on_low_battery,
                    )
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
            }
        }

        if let Some(session) = &self.session {
            if self.capabilities.idle_delay {
                session
                    .settings
                    .set_uint(KEY_IDLE_DELAY, snapshot.idle_delay)
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
            }
        }

        if let Some(screensaver) = &self.screensaver {
            if self.capabilities.idle_activation_enabled {
                screensaver
                    .settings
                    .set_boolean(KEY_IDLE_ACTIVATION_ENABLED, snapshot.idle_activation_enabled)
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
            }

            if self.capabilities.lock_enabled {
                screensaver
                    .settings
                    .set_boolean(KEY_LOCK_ENABLED, snapshot.lock_enabled)
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
            }

            if self.capabilities.lock_delay {
                screensaver
                    .settings
                    .set_uint(KEY_LOCK_DELAY, snapshot.lock_delay)
                    .map_err(|error| anyhow::anyhow!("{error}"))?;
            }
        }

        Ok(())
    }

    pub async fn run(
        &self,
        events: mpsc::Sender<PowerPolicyEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let mut tracker = PowerPolicyTracker::default();
        let initial = self.load()?;
        if let Some(event) = tracker.push(initial) {
            let _ = events.send(event).await;
        }

        let mut ticker = tokio::time::interval(self.poll_interval);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = ticker.tick() => {
                    Settings::sync();
                    let snapshot = self.load()?;
                    if let Some(event) = tracker.push(snapshot) {
                        if events.send(event).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn from_schema_source(
        source: Option<SettingsSchemaSource>,
        poll_interval: Duration,
    ) -> Self {
        let power = SettingsBinding::lookup(source.as_ref(), POWER_SCHEMA);
        let session = SettingsBinding::lookup(source.as_ref(), SESSION_SCHEMA);
        let screensaver = SettingsBinding::lookup(source.as_ref(), SCREENSAVER_SCHEMA);

        Self {
            capabilities: PowerPolicyCapabilities {
                sleep_inactive_battery_timeout: power
                    .as_ref()
                    .is_some_and(|binding| binding.supports(KEY_SLEEP_INACTIVE_BATTERY_TIMEOUT)),
                sleep_inactive_battery_action: power
                    .as_ref()
                    .is_some_and(|binding| binding.supports(KEY_SLEEP_INACTIVE_BATTERY_TYPE)),
                sleep_inactive_ac_timeout: power
                    .as_ref()
                    .is_some_and(|binding| binding.supports(KEY_SLEEP_INACTIVE_AC_TIMEOUT)),
                sleep_inactive_ac_action: power
                    .as_ref()
                    .is_some_and(|binding| binding.supports(KEY_SLEEP_INACTIVE_AC_TYPE)),
                power_saver_profile_on_low_battery: power.as_ref().is_some_and(|binding| {
                    binding.supports(KEY_POWER_SAVER_PROFILE_ON_LOW_BATTERY)
                }),
                idle_delay: session
                    .as_ref()
                    .is_some_and(|binding| binding.supports(KEY_IDLE_DELAY)),
                idle_activation_enabled: screensaver
                    .as_ref()
                    .is_some_and(|binding| binding.supports(KEY_IDLE_ACTIVATION_ENABLED)),
                lock_enabled: screensaver
                    .as_ref()
                    .is_some_and(|binding| binding.supports(KEY_LOCK_ENABLED)),
                lock_delay: screensaver
                    .as_ref()
                    .is_some_and(|binding| binding.supports(KEY_LOCK_DELAY)),
            },
            power,
            session,
            screensaver,
            poll_interval,
        }
    }
}

#[derive(Clone)]
struct SettingsBinding {
    settings: Settings,
    schema: SettingsSchema,
}

impl SettingsBinding {
    fn lookup(source: Option<&SettingsSchemaSource>, schema_id: &str) -> Option<Self> {
        let schema = source?.lookup(schema_id, true)?;
        let settings = Settings::new_full(&schema, Option::<&gio::SettingsBackend>::None, None);
        Some(Self { settings, schema })
    }

    fn supports(&self, key: &str) -> bool {
        self.schema.has_key(key)
    }
}

#[derive(Default)]
struct PowerPolicyTracker {
    last: Option<PowerPolicySnapshot>,
}

impl PowerPolicyTracker {
    fn push(&mut self, snapshot: PowerPolicySnapshot) -> Option<PowerPolicyEvent> {
        if self.last.as_ref() == Some(&snapshot) {
            return None;
        }

        self.last = Some(snapshot.clone());
        Some(PowerPolicyEvent::Changed(snapshot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_policy_action_round_trips_known_values() {
        let values = [
            ("blank", PowerPolicyAction::Blank),
            ("suspend", PowerPolicyAction::Suspend),
            ("hibernate", PowerPolicyAction::Hibernate),
            ("shutdown", PowerPolicyAction::Shutdown),
            ("interactive", PowerPolicyAction::Interactive),
            ("nothing", PowerPolicyAction::Nothing),
            ("logout", PowerPolicyAction::Logout),
        ];

        for (raw, expected) in values {
            assert_eq!(PowerPolicyAction::from_gsettings(raw), expected);
            assert_eq!(expected.as_gsettings(), Cow::Borrowed(raw));
        }
    }

    #[test]
    fn power_policy_action_preserves_unknown_values() {
        let action = PowerPolicyAction::from_gsettings("custom-action");
        assert_eq!(action, PowerPolicyAction::Other("custom-action".into()));
        assert_eq!(action.as_gsettings(), Cow::Borrowed("custom-action"));
    }

    #[test]
    fn tracker_emits_only_on_snapshot_changes() {
        let mut tracker = PowerPolicyTracker::default();
        let snapshot = PowerPolicySnapshot {
            idle_delay: 300,
            ..PowerPolicySnapshot::default()
        };

        assert_eq!(
            tracker.push(snapshot.clone()),
            Some(PowerPolicyEvent::Changed(snapshot.clone()))
        );
        assert_eq!(tracker.push(snapshot.clone()), None);

        let changed = PowerPolicySnapshot {
            idle_delay: 600,
            ..snapshot.clone()
        };
        assert_eq!(
            tracker.push(changed.clone()),
            Some(PowerPolicyEvent::Changed(changed))
        );
    }

    #[test]
    fn provider_without_schemas_reports_capabilities_as_disabled() {
        let provider = PowerPolicySettings::from_schema_source(None, DEFAULT_POLL_INTERVAL);
        let snapshot = provider.load().expect("load should succeed without schemas");

        assert_eq!(snapshot, PowerPolicySnapshot::default());
        assert_eq!(provider.capabilities(), &PowerPolicyCapabilities::default());
    }
}
