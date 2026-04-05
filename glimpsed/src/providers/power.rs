use std::pin::Pin;

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};
use crate::providers::dbus_props::DbusPropertyGroup;

const NAME: &str = "power";
const TOPICS: &[&str] = &["power.profiles", "power.actions"];
const METHODS: &[&str] = &[
    "power.set_profile",
    "power.suspend",
    "power.hibernate",
    "power.reboot",
    "power.poweroff",
    "power.lock",
];

#[derive(Debug, Clone, Serialize, Default)]
struct PowerProfiles {
    active: String,
    icon_name: &'static str,
    available: Vec<String>,
    performance_degraded: String,
}

#[derive(Debug, Clone, Serialize)]
struct PowerActions {
    can_suspend: String,
    can_hibernate: String,
    can_reboot: String,
    can_poweroff: String,
}

struct PowerProvider {
    profiles: PowerProfiles,
    actions: PowerActions,
}

impl Provider for PowerProvider {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }

    fn run(
        &mut self,
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            tracing::info!("power: starting");
            let conn = zbus::Connection::system().await?;
            let logind = DbusPropertyGroup::new(
                &conn,
                "org.freedesktop.login1",
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
            )
            .await?;

            self.actions = PowerActions {
                can_suspend: logind.call("CanSuspend", &()).await.unwrap_or_default(),
                can_hibernate: logind.call("CanHibernate", &()).await.unwrap_or_default(),
                can_reboot: logind.call("CanReboot", &()).await.unwrap_or_default(),
                can_poweroff: logind.call("CanPowerOff", &()).await.unwrap_or_default(),
            };

            // Read power profiles (may not be available).
            let profiles_proxy = DbusPropertyGroup::new(
                &conn,
                "net.hadess.PowerProfiles",
                "/net/hadess/PowerProfiles",
                "net.hadess.PowerProfiles",
            )
            .await;

            if let Ok(ref pp) = profiles_proxy {
                self.read_profiles(pp).await;
                tracing::info!(
                    active = %self.profiles.active,
                    available = ?self.profiles.available,
                    "power: profiles loaded"
                );
            } else {
                tracing::warn!("power: power-profiles-daemon not available");
            }
            let mut profile_changes = match &profiles_proxy {
                Ok(pp) => Some(pp.stream_changes().await?),
                Err(_) => None,
            };

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req, &logind, profiles_proxy.as_ref().ok()).await;
                    }
                    Some(_) = async {
                        match &mut profile_changes {
                            Some(stream) => stream.next().await,
                            None => std::future::pending().await,
                        }
                    } => {
                        if let Ok(ref pp) = profiles_proxy {
                            self.read_profiles(pp).await;
                            let _ = events.send(ProviderEvent {
                                topic: "power.profiles".into(),
                                data: serde_json::to_value(&self.profiles).unwrap_or_default(),
                            }).await;
                        }
                    }
                }
            }

            Ok(())
        })
    }
}

impl PowerProvider {
    async fn read_profiles(&mut self, pp: &DbusPropertyGroup) {
        use std::collections::HashMap;
        use zbus::zvariant::OwnedValue;

        self.profiles.active = pp.get_uncached("ActiveProfile").await.unwrap_or_default();
        self.profiles.icon_name = profile_icon(&self.profiles.active);
        self.profiles.performance_degraded =
            pp.get_uncached("PerformanceDegraded").await.unwrap_or_default();

        let raw: Vec<HashMap<String, OwnedValue>> =
            pp.get_uncached("Profiles").await.unwrap_or_default();
        self.profiles.available = raw
            .iter()
            .filter_map(|d| {
                d.get("Profile").and_then(|v| {
                    use zbus::zvariant::Value;
                    match &**v {
                        Value::Str(s) => Some(s.to_string()),
                        _ => None,
                    }
                })
            })
            .collect();
    }

    async fn handle_request(
        &self,
        req: ProviderRequest,
        logind: &DbusPropertyGroup,
        profiles_proxy: Option<&DbusPropertyGroup>,
    ) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "power.profiles" => serde_json::to_value(&self.profiles).ok(),
                    "power.actions" => serde_json::to_value(&self.actions).ok(),
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call { method, params, reply } => {
                let result = match method.as_str() {
                    "power.set_profile" => {
                        let profile = params.as_str().or_else(|| params["profile"].as_str());
                        tracing::info!("setting power profile to {}", profile.unwrap_or("?"));
                        match (profile, profiles_proxy) {
                            (Some(p), Some(pp)) => pp
                                .set("ActiveProfile", p.to_owned())
                                .await
                                .map(|()| json!(null))
                                .map_err(|e| anyhow::anyhow!("{e}")),
                            (None, _) => Err(anyhow::anyhow!("missing 'profile' param")),
                            (_, None) => Err(anyhow::anyhow!("power profiles not available")),
                        }
                    }
                    "power.suspend" => {
                        tracing::info!("suspending system");
                        logind_action(logind, "Suspend").await
                    }
                    "power.hibernate" => {
                        tracing::info!("hibernating system");
                        logind_action(logind, "Hibernate").await
                    }
                    "power.reboot" => {
                        tracing::info!("rebooting system");
                        logind_action(logind, "Reboot").await
                    }
                    "power.poweroff" => {
                        tracing::info!("powering off system");
                        logind_action(logind, "PowerOff").await
                    }
                    "power.lock" => {
                        tracing::info!("locking session");
                        logind.call_void("LockSessions", &()).await
                            .map(|()| json!(null))
                            .map_err(|e| anyhow::anyhow!("{e}"))
                    }
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                if let Err(ref e) = result {
                    tracing::warn!(method = %method, error = %e, "power: call failed");
                }
                let _ = reply.send(result);
            }
        }
    }
}

async fn logind_action(
    logind: &DbusPropertyGroup,
    method: &str,
) -> anyhow::Result<serde_json::Value> {
    logind
        .call_void(method, &(false,))
        .await
        .map(|()| json!(null))
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn profile_icon(profile: &str) -> &'static str {
    match profile {
        "power-saver" => "power-profile-power-saver-symbolic",
        "balanced" => "power-profile-balanced-symbolic",
        "performance" => "power-profile-performance-symbolic",
        _ => "power-profile-balanced-symbolic",
    }
}

pub struct PowerProviderFactory;

impl ProviderFactory for PowerProviderFactory {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }
    fn create(&self) -> Box<dyn Provider> {
        Box::new(PowerProvider {
            profiles: PowerProfiles::default(),
            actions: PowerActions {
                can_suspend: String::new(),
                can_hibernate: String::new(),
                can_reboot: String::new(),
                can_poweroff: String::new(),
            },
        })
    }
}
