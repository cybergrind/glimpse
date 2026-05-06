use std::{
    fmt,
    sync::{Arc, Mutex},
};

use tokio::sync::{oneshot, watch};
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;
use zbus::zvariant::ObjectPath;

use glimpse_core::dbus::bluez::Device1Proxy;

const AGENT_PATH: &str = "/org/bluez/glimpse/agent";

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct BluetoothPromptId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothPromptKind {
    Confirm { passkey: u32 },
    AuthorizePairing,
    AuthorizeService { uuid: String },
    RequestPin,
    RequestPasskey,
    DisplayPin { pincode: String },
    DisplayPasskey { passkey: u32, entered: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothPrompt {
    pub id: BluetoothPromptId,
    pub device_path: String,
    pub device_label: String,
    pub kind: BluetoothPromptKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothPromptReply {
    Confirm,
    Pin(String),
    Passkey(u32),
    Cancel,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
pub(crate) enum BluezError {
    Rejected(String),
    Canceled(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceAuthorizationPolicy {
    Prompt,
    Accept,
    TrustAndAccept,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PromptRegistryError {
    Busy,
}

impl fmt::Display for PromptRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("a bluetooth prompt is already active"),
        }
    }
}

struct PromptRegistry {
    next_prompt_id: u64,
    current: Option<PendingPrompt>,
    prompt_tx: watch::Sender<Option<BluetoothPrompt>>,
}

struct PendingPrompt {
    prompt: BluetoothPrompt,
    reply_tx: Option<oneshot::Sender<BluetoothPromptReply>>,
}

impl PromptRegistry {
    fn new(prompt_tx: watch::Sender<Option<BluetoothPrompt>>) -> Self {
        Self {
            next_prompt_id: 1,
            current: None,
            prompt_tx,
        }
    }

    #[cfg(test)]
    pub fn current_prompt(&self) -> Option<BluetoothPrompt> {
        self.current.as_ref().map(|current| current.prompt.clone())
    }

    fn request_prompt(
        &mut self,
        device_path: String,
        device_label: String,
        kind: BluetoothPromptKind,
    ) -> Result<(BluetoothPromptId, oneshot::Receiver<BluetoothPromptReply>), PromptRegistryError>
    {
        if self.current.is_some() {
            tracing::debug!(
                device = device_path,
                kind = prompt_kind_name(&kind),
                "bluetooth-agent: prompt request rejected because another prompt is active"
            );
            return Err(PromptRegistryError::Busy);
        }

        let id = self.allocate_prompt_id();
        let (reply_tx, reply_rx) = oneshot::channel();
        tracing::debug!(
            device = device_path,
            prompt_id = id.0,
            kind = prompt_kind_name(&kind),
            "bluetooth-agent: prompt request registered"
        );
        self.publish_prompt(
            BluetoothPrompt {
                id,
                device_path,
                device_label,
                kind,
            },
            Some(reply_tx),
        );

        Ok((id, reply_rx))
    }

    fn publish_display_prompt(
        &mut self,
        device_path: String,
        device_label: String,
        kind: BluetoothPromptKind,
    ) -> Result<BluetoothPromptId, PromptRegistryError> {
        if let Some(current) = self.current.as_ref() {
            if current.reply_tx.is_some() {
                tracing::debug!(
                    device = device_path,
                    active_prompt_id = current.prompt.id.0,
                    active_kind = prompt_kind_name(&current.prompt.kind),
                    display_kind = prompt_kind_name(&kind),
                    "bluetooth-agent: display prompt skipped because interactive prompt is active"
                );
                return Err(PromptRegistryError::Busy);
            }

            let current_prompt = &current.prompt;
            if current_prompt.device_path == device_path
                && display_prompt_kind_matches(&current_prompt.kind, &kind)
            {
                let id = current_prompt.id;
                tracing::debug!(
                    device = device_path,
                    prompt_id = id.0,
                    kind = prompt_kind_name(&kind),
                    "bluetooth-agent: display prompt updated"
                );
                self.publish_prompt(
                    BluetoothPrompt {
                        id,
                        device_path,
                        device_label,
                        kind,
                    },
                    None,
                );
                return Ok(id);
            }
        }

        let id = self.allocate_prompt_id();
        tracing::debug!(
            device = device_path,
            prompt_id = id.0,
            kind = prompt_kind_name(&kind),
            "bluetooth-agent: display prompt published"
        );
        self.publish_prompt(
            BluetoothPrompt {
                id,
                device_path,
                device_label,
                kind,
            },
            None,
        );
        Ok(id)
    }

    fn complete(&mut self, id: BluetoothPromptId, reply: BluetoothPromptReply) -> bool {
        if self.current.as_ref().map(|current| current.prompt.id) != Some(id) {
            tracing::debug!(
                prompt_id = id.0,
                reply = prompt_reply_name(&reply),
                "bluetooth-agent: prompt reply ignored because prompt is not active"
            );
            return false;
        }

        let current = self.current.take();
        if let Some(current) = current {
            tracing::debug!(
                prompt_id = id.0,
                kind = prompt_kind_name(&current.prompt.kind),
                reply = prompt_reply_name(&reply),
                has_reply_channel = current.reply_tx.is_some(),
                "bluetooth-agent: prompt completed"
            );
            if let Some(reply_tx) = current.reply_tx {
                let _ = reply_tx.send(reply);
            }
        }

        let _ = self.prompt_tx.send(None);
        true
    }

    fn cancel_current(&mut self) -> bool {
        let Some(current) = self.current.take() else {
            tracing::debug!("bluetooth-agent: cancel ignored because no prompt is active");
            return false;
        };

        tracing::debug!(
            prompt_id = current.prompt.id.0,
            kind = prompt_kind_name(&current.prompt.kind),
            has_reply_channel = current.reply_tx.is_some(),
            "bluetooth-agent: active prompt cancelled"
        );
        if let Some(reply_tx) = current.reply_tx {
            let _ = reply_tx.send(BluetoothPromptReply::Cancel);
        }

        let _ = self.prompt_tx.send(None);
        true
    }

    fn allocate_prompt_id(&mut self) -> BluetoothPromptId {
        let id = BluetoothPromptId(self.next_prompt_id);
        self.next_prompt_id += 1;
        id
    }

    fn publish_prompt(
        &mut self,
        prompt: BluetoothPrompt,
        reply_tx: Option<oneshot::Sender<BluetoothPromptReply>>,
    ) {
        self.current = Some(PendingPrompt {
            prompt: prompt.clone(),
            reply_tx,
        });
        let _ = self.prompt_tx.send(Some(prompt));
    }
}

fn display_prompt_kind_matches(left: &BluetoothPromptKind, right: &BluetoothPromptKind) -> bool {
    matches!(
        (left, right),
        (
            BluetoothPromptKind::DisplayPin { .. },
            BluetoothPromptKind::DisplayPin { .. }
        ) | (
            BluetoothPromptKind::DisplayPasskey { .. },
            BluetoothPromptKind::DisplayPasskey { .. }
        )
    )
}

fn prompt_kind_name(kind: &BluetoothPromptKind) -> &'static str {
    match kind {
        BluetoothPromptKind::Confirm { .. } => "confirm",
        BluetoothPromptKind::AuthorizePairing => "authorize-pairing",
        BluetoothPromptKind::AuthorizeService { .. } => "authorize-service",
        BluetoothPromptKind::RequestPin => "request-pin",
        BluetoothPromptKind::RequestPasskey => "request-passkey",
        BluetoothPromptKind::DisplayPin { .. } => "display-pin",
        BluetoothPromptKind::DisplayPasskey { .. } => "display-passkey",
    }
}

fn prompt_reply_name(reply: &BluetoothPromptReply) -> &'static str {
    match reply {
        BluetoothPromptReply::Confirm => "confirm",
        BluetoothPromptReply::Pin(_) => "pin",
        BluetoothPromptReply::Passkey(_) => "passkey",
        BluetoothPromptReply::Cancel => "cancel",
    }
}

fn service_authorization_policy(paired: bool, trusted: bool) -> ServiceAuthorizationPolicy {
    match (paired, trusted) {
        (true, true) => ServiceAuthorizationPolicy::Accept,
        (true, false) => ServiceAuthorizationPolicy::TrustAndAccept,
        (false, _) => ServiceAuthorizationPolicy::Prompt,
    }
}

#[derive(Clone)]
pub struct BluetoothAgent {
    registry: Arc<Mutex<PromptRegistry>>,
    conn: zbus::Connection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentDefaultStatus {
    Default,
    RegisteredOnly,
}

impl BluetoothAgent {
    pub fn new(conn: zbus::Connection) -> (Self, watch::Receiver<Option<BluetoothPrompt>>) {
        let (prompt_tx, prompt_rx) = watch::channel(None);
        let registry = Arc::new(Mutex::new(PromptRegistry::new(prompt_tx)));

        (Self { registry, conn }, prompt_rx)
    }

    pub fn complete_prompt(&self, id: BluetoothPromptId, reply: BluetoothPromptReply) -> bool {
        self.registry
            .lock()
            .expect("bluetooth prompt registry poisoned")
            .complete(id, reply)
    }

    pub fn cancel_prompt(&self) -> bool {
        self.registry
            .lock()
            .expect("bluetooth prompt registry poisoned")
            .cancel_current()
    }

    pub async fn register(&self) -> zbus::Result<AgentDefaultStatus> {
        tracing::debug!(path = AGENT_PATH, "bluetooth-agent: registering object");
        let _ = self
            .conn
            .object_server()
            .remove::<Self, _>(AGENT_PATH)
            .await;
        self.conn
            .object_server()
            .at(AGENT_PATH, self.clone())
            .await?;

        let agent_mgr = zbus::Proxy::new(
            &self.conn,
            "org.bluez",
            "/org/bluez",
            "org.bluez.AgentManager1",
        )
        .await?;
        let path = ObjectPath::try_from(AGENT_PATH)?;

        tracing::debug!(path = %path, "bluetooth-agent: unregistering stale agent before register");
        let _ = agent_mgr
            .call::<_, _, ()>("UnregisterAgent", &(&path,))
            .await;

        tracing::debug!(path = %path, capability = "KeyboardDisplay", "bluetooth-agent: registering with bluez");
        agent_mgr
            .call::<_, _, ()>("RegisterAgent", &(&path, "KeyboardDisplay"))
            .await
            .map_err(|error| {
                zbus::Error::Failure(format!("failed to register bluetooth agent: {error}"))
            })?;

        tracing::debug!(path = %path, "bluetooth-agent: requesting default agent");
        let default_status = match agent_mgr
            .call::<_, _, ()>("RequestDefaultAgent", &(&path,))
            .await
        {
            Ok(()) => AgentDefaultStatus::Default,
            Err(error) => {
                tracing::warn!(error = %error, "bluetooth-agent: failed to become default agent");
                AgentDefaultStatus::RegisteredOnly
            }
        };

        tracing::info!(default = ?default_status, "bluetooth-agent: registered");
        Ok(default_status)
    }

    pub async fn unregister(&self) -> zbus::Result<()> {
        tracing::debug!(path = AGENT_PATH, "bluetooth-agent: unregistering");
        let agent_mgr = zbus::Proxy::new(
            &self.conn,
            "org.bluez",
            "/org/bluez",
            "org.bluez.AgentManager1",
        )
        .await?;
        let path = ObjectPath::try_from(AGENT_PATH)?;

        let _ = agent_mgr
            .call::<_, _, ()>("UnregisterAgent", &(&path,))
            .await;
        let _ = self
            .conn
            .object_server()
            .remove::<Self, _>(AGENT_PATH)
            .await;
        self.cancel_prompt();
        tracing::info!("bluetooth-agent: unregistered");
        Ok(())
    }

    pub(crate) async fn device_label(&self, device_path: &str) -> String {
        let Ok(builder) = Device1Proxy::builder(&self.conn).path(device_path) else {
            return device_path.to_owned();
        };
        let Ok(proxy) = builder.build().await else {
            return device_path.to_owned();
        };
        let alias = proxy.alias().await.unwrap_or_default();
        if alias.is_empty() {
            device_path.to_owned()
        } else {
            alias
        }
    }

    pub(crate) async fn trust_if_paired(&self, device_path: &str) -> zbus::fdo::Result<bool> {
        let builder = Device1Proxy::builder(&self.conn)
            .path(device_path)
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        let proxy = builder
            .build()
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        let paired = proxy.paired().await.unwrap_or(false);
        let trusted = proxy.trusted().await.unwrap_or(false);

        match service_authorization_policy(paired, trusted) {
            ServiceAuthorizationPolicy::Prompt => {
                tracing::debug!(
                    device = device_path,
                    "bluetooth-agent: service authorization needs prompt because device is not paired"
                );
                Ok(false)
            }
            ServiceAuthorizationPolicy::Accept => Ok(true),
            ServiceAuthorizationPolicy::TrustAndAccept => {
                tracing::debug!(
                    device = device_path,
                    "bluetooth-agent: trusting paired device for service authorization"
                );
                proxy
                    .set_trusted(true)
                    .await
                    .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
                Ok(true)
            }
        }
    }

    pub(crate) async fn publish_display_prompt(
        &self,
        device_path: &str,
        kind: BluetoothPromptKind,
    ) -> Result<BluetoothPromptId, PromptRegistryError> {
        let label = self.device_label(device_path).await;
        self.registry
            .lock()
            .expect("bluetooth prompt registry poisoned")
            .publish_display_prompt(device_path.to_owned(), label, kind)
    }

    pub(crate) async fn request_reply(
        &self,
        device_path: &str,
        kind: BluetoothPromptKind,
    ) -> Result<BluetoothPromptReply, BluezError> {
        let label = self.device_label(device_path).await;
        let kind_name = prompt_kind_name(&kind);
        let (id, reply_rx) = {
            let mut registry = self
                .registry
                .lock()
                .expect("bluetooth prompt registry poisoned");
            registry
                .request_prompt(device_path.to_owned(), label, kind)
                .map_err(|error| BluezError::Rejected(error.to_string()))?
        };

        tracing::info!(
            device = device_path,
            prompt_id = id.0,
            kind = kind_name,
            "bluetooth-agent: prompt emitted"
        );

        let reply = reply_rx
            .await
            .map_err(|_| BluezError::Rejected("bluetooth prompt reply channel closed".into()))?;
        tracing::debug!(
            device = device_path,
            prompt_id = id.0,
            kind = kind_name,
            reply = prompt_reply_name(&reply),
            "bluetooth-agent: prompt reply received"
        );
        Ok(reply)
    }

    pub(crate) async fn request_string_reply(
        &self,
        device_path: &str,
        kind: BluetoothPromptKind,
        label: &'static str,
    ) -> Result<String, BluezError> {
        match self.request_reply(device_path, kind).await? {
            BluetoothPromptReply::Pin(value) => {
                tracing::info!(
                    device = device_path,
                    label,
                    "bluetooth-agent: prompt accepted"
                );
                Ok(value)
            }
            BluetoothPromptReply::Cancel => Err(BluezError::Canceled("cancelled by user".into())),
            _ => Err(BluezError::Rejected(
                "unexpected bluetooth prompt reply".into(),
            )),
        }
    }

    pub(crate) async fn request_passkey_reply(
        &self,
        device_path: &str,
        kind: BluetoothPromptKind,
        label: &'static str,
    ) -> Result<u32, BluezError> {
        match self.request_reply(device_path, kind).await? {
            BluetoothPromptReply::Passkey(passkey) => {
                tracing::info!(
                    device = device_path,
                    label,
                    passkey,
                    "bluetooth-agent: prompt accepted"
                );
                Ok(passkey)
            }
            BluetoothPromptReply::Cancel => Err(BluezError::Canceled("cancelled by user".into())),
            _ => Err(BluezError::Rejected(
                "unexpected bluetooth prompt reply".into(),
            )),
        }
    }
}

#[derive(Clone)]
pub struct BluetoothAgentHandle {
    agent: BluetoothAgent,
    prompt_rx: watch::Receiver<Option<BluetoothPrompt>>,
}

impl BluetoothAgentHandle {
    pub fn subscribe(&self) -> watch::Receiver<Option<BluetoothPrompt>> {
        self.prompt_rx.clone()
    }

    pub fn reply(&self, id: BluetoothPromptId, reply: BluetoothPromptReply) -> bool {
        self.agent.complete_prompt(id, reply)
    }
}

pub struct BluetoothAgentRuntime {
    agent: BluetoothAgent,
}

impl BluetoothAgentRuntime {
    pub fn new(conn: zbus::Connection) -> (Self, BluetoothAgentHandle) {
        let (agent, prompt_rx) = BluetoothAgent::new(conn);
        let handle = BluetoothAgentHandle {
            agent: agent.clone(),
            prompt_rx,
        };

        (Self { agent }, handle)
    }

    pub async fn run(self, cancel: CancellationToken) {
        loop {
            match self.agent.register().await {
                Ok(default_status) => {
                    if default_status == AgentDefaultStatus::RegisteredOnly {
                        tracing::warn!(
                            "bluetooth-agent: registered but is not default; prompts may open elsewhere"
                        );
                    }

                    cancel.cancelled().await;
                    if let Err(error) = self.agent.unregister().await {
                        tracing::warn!(error = %error, "bluetooth-agent: unregister failed");
                    }
                    break;
                }
                Err(error) => {
                    tracing::warn!(error = %error, "bluetooth-agent: register failed, retrying");
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = sleep(Duration::from_secs(2)) => {}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_authorization_policy_matches_gnome_trust_behavior() {
        assert_eq!(
            service_authorization_policy(true, false),
            ServiceAuthorizationPolicy::TrustAndAccept
        );
        assert_eq!(
            service_authorization_policy(true, true),
            ServiceAuthorizationPolicy::Accept
        );
        assert_eq!(
            service_authorization_policy(false, false),
            ServiceAuthorizationPolicy::Prompt
        );
        assert_eq!(
            service_authorization_policy(false, true),
            ServiceAuthorizationPolicy::Prompt
        );
    }

    #[tokio::test]
    async fn prompt_registry_completes_matching_request() {
        let (prompt_tx, prompt_rx) = watch::channel(None);
        let mut registry = PromptRegistry::new(prompt_tx);

        let (prompt_id, reply_rx) = registry
            .request_prompt(
                "/org/bluez/hci0/dev_AA_BB".into(),
                "Headphones".into(),
                BluetoothPromptKind::RequestPin,
            )
            .expect("prompt request");

        assert_eq!(
            registry.current_prompt().as_ref().map(|prompt| prompt.id),
            Some(prompt_id)
        );
        assert_eq!(
            prompt_rx.borrow().as_ref().map(|prompt| prompt.id),
            Some(prompt_id)
        );

        assert!(registry.complete(prompt_id, BluetoothPromptReply::Pin("1234".into())));
        assert_eq!(
            reply_rx.await.expect("reply"),
            BluetoothPromptReply::Pin("1234".into())
        );
        assert!(registry.current_prompt().is_none());
        assert!(prompt_rx.borrow().is_none());
    }

    #[test]
    fn prompt_registry_publishes_display_prompt_state() {
        let (prompt_tx, prompt_rx) = watch::channel(None);
        let mut registry = PromptRegistry::new(prompt_tx);

        let prompt_id = registry
            .publish_display_prompt(
                "/org/bluez/hci0/dev_AA_BB".into(),
                "Headphones".into(),
                BluetoothPromptKind::DisplayPasskey {
                    passkey: 123_456,
                    entered: 3,
                },
            )
            .expect("display prompt");

        assert_eq!(
            prompt_rx.borrow().as_ref().map(|prompt| prompt.id),
            Some(prompt_id)
        );
        assert!(registry.complete(prompt_id, BluetoothPromptReply::Cancel));
    }

    #[test]
    fn prompt_registry_updates_existing_display_prompt() {
        let (prompt_tx, prompt_rx) = watch::channel(None);
        let mut registry = PromptRegistry::new(prompt_tx);

        let first_id = registry
            .publish_display_prompt(
                "/org/bluez/hci0/dev_AA_BB".into(),
                "Headphones".into(),
                BluetoothPromptKind::DisplayPasskey {
                    passkey: 123_456,
                    entered: 0,
                },
            )
            .expect("first display prompt");
        let second_id = registry
            .publish_display_prompt(
                "/org/bluez/hci0/dev_AA_BB".into(),
                "Headphones".into(),
                BluetoothPromptKind::DisplayPasskey {
                    passkey: 123_456,
                    entered: 3,
                },
            )
            .expect("updated display prompt");

        assert_eq!(first_id, second_id);
        assert_eq!(
            prompt_rx
                .borrow()
                .as_ref()
                .map(|prompt| prompt.kind.clone()),
            Some(BluetoothPromptKind::DisplayPasskey {
                passkey: 123_456,
                entered: 3,
            })
        );
    }
}
