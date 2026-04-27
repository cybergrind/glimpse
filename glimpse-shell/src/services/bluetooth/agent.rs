use std::{
    fmt,
    sync::{Arc, Mutex},
};

use tokio::sync::{oneshot, watch};
use zbus::zvariant::ObjectPath;

use crate::{
    dbus::bluez::Device1Proxy,
    services::bluetooth::{
        BluetoothPrompt, BluetoothPromptId, BluetoothPromptKind, BluetoothPromptReply,
    },
};

const AGENT_PATH: &str = "/org/bluez/glimpse/agent";

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum BluezError {
    Rejected(String),
    Canceled(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptRegistryError {
    Busy,
}

impl fmt::Display for PromptRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("a bluetooth prompt is already active"),
        }
    }
}

pub struct PromptRegistry {
    next_prompt_id: u64,
    current: Option<PendingPrompt>,
    prompt_tx: watch::Sender<Option<BluetoothPrompt>>,
}

struct PendingPrompt {
    prompt: BluetoothPrompt,
    reply_tx: Option<oneshot::Sender<BluetoothPromptReply>>,
}

impl PromptRegistry {
    pub fn new(prompt_tx: watch::Sender<Option<BluetoothPrompt>>) -> Self {
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

    pub fn request_prompt(
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

    pub fn publish_display_prompt(
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

    pub fn complete(&mut self, id: BluetoothPromptId, reply: BluetoothPromptReply) -> bool {
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

    pub fn cancel_current(&mut self) -> bool {
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
        BluetoothPromptReply::Reject => "reject",
        BluetoothPromptReply::Pin(_) => "pin",
        BluetoothPromptReply::Passkey(_) => "passkey",
        BluetoothPromptReply::Cancel => "cancel",
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

    async fn device_label(&self, device_path: &str) -> String {
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

    async fn request_reply(
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

    async fn request_string_reply(
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
            BluetoothPromptReply::Reject => Err(BluezError::Rejected("rejected by user".into())),
            _ => Err(BluezError::Rejected(
                "unexpected bluetooth prompt reply".into(),
            )),
        }
    }

    async fn request_passkey_reply(
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
            BluetoothPromptReply::Reject => Err(BluezError::Rejected("rejected by user".into())),
            _ => Err(BluezError::Rejected(
                "unexpected bluetooth prompt reply".into(),
            )),
        }
    }
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl BluetoothAgent {
    fn release(&self) {
        tracing::debug!("bluetooth-agent: released by bluez");
        tracing::info!("bluetooth-agent: released");
    }

    async fn request_confirmation(
        &self,
        device: ObjectPath<'_>,
        passkey: u32,
    ) -> Result<(), BluezError> {
        let device_path = device.as_str().to_owned();
        tracing::info!(
            device = device_path,
            passkey,
            "bluetooth-agent: confirmation requested"
        );
        tracing::debug!(
            device = device_path,
            prompt_kind = "confirm",
            "bluetooth-agent: confirmation prompt requested"
        );
        match self
            .request_reply(&device_path, BluetoothPromptKind::Confirm { passkey })
            .await?
        {
            BluetoothPromptReply::Confirm => Ok(()),
            BluetoothPromptReply::Cancel => Err(BluezError::Canceled("cancelled by user".into())),
            _ => Err(BluezError::Rejected("rejected by user".into())),
        }
    }

    async fn request_authorization(&self, device: ObjectPath<'_>) -> zbus::fdo::Result<()> {
        let device_path = device.as_str().to_owned();
        tracing::info!(
            device = device_path,
            "bluetooth-agent: authorizing pairing request"
        );
        tracing::debug!(
            device = device_path,
            prompt_kind = "authorize-pairing",
            "bluetooth-agent: authorization prompt requested"
        );
        match self
            .request_reply(&device_path, BluetoothPromptKind::AuthorizePairing)
            .await
        {
            Ok(BluetoothPromptReply::Confirm) => Ok(()),
            Ok(BluetoothPromptReply::Cancel) => {
                Err(zbus::fdo::Error::Failed("cancelled by user".into()))
            }
            Ok(_) => Err(zbus::fdo::Error::Failed("rejected by user".into())),
            Err(error) => Err(zbus::fdo::Error::Failed(error.to_string())),
        }
    }

    async fn authorize_service(&self, device: ObjectPath<'_>, uuid: &str) -> zbus::fdo::Result<()> {
        let device_path = device.as_str().to_owned();
        tracing::info!(
            device = device_path,
            uuid,
            "bluetooth-agent: authorizing service"
        );
        tracing::debug!(
            device = device_path,
            uuid,
            prompt_kind = "authorize-service",
            "bluetooth-agent: service authorization prompt requested"
        );
        match self
            .request_reply(
                &device_path,
                BluetoothPromptKind::AuthorizeService { uuid: uuid.into() },
            )
            .await
        {
            Ok(BluetoothPromptReply::Confirm) => Ok(()),
            Ok(BluetoothPromptReply::Cancel) => {
                Err(zbus::fdo::Error::Failed("cancelled by user".into()))
            }
            Ok(_) => Err(zbus::fdo::Error::Failed("rejected by user".into())),
            Err(error) => Err(zbus::fdo::Error::Failed(error.to_string())),
        }
    }

    async fn request_passkey(&self, device: ObjectPath<'_>) -> Result<u32, BluezError> {
        let device_path = device.as_str().to_owned();
        tracing::debug!(
            device = device_path,
            prompt_kind = "request-passkey",
            "bluetooth-agent: passkey prompt requested"
        );
        self.request_passkey_reply(&device_path, BluetoothPromptKind::RequestPasskey, "passkey")
            .await
    }

    async fn display_passkey(&self, device: ObjectPath<'_>, passkey: u32, entered: u16) {
        let device_path = device.as_str().to_owned();
        tracing::debug!(
            device = device_path,
            entered,
            prompt_kind = "display-passkey",
            "bluetooth-agent: display passkey requested"
        );
        let label = self.device_label(&device_path).await;
        let prompt_id = {
            let mut registry = self
                .registry
                .lock()
                .expect("bluetooth prompt registry poisoned");
            registry.publish_display_prompt(
                device_path.clone(),
                label,
                BluetoothPromptKind::DisplayPasskey { passkey, entered },
            )
        };
        let Ok(prompt_id) = prompt_id else {
            tracing::warn!(
                device = device_path,
                "bluetooth-agent: display passkey prompt skipped because another prompt is active"
            );
            return;
        };
        tracing::info!(
            device = device_path,
            prompt_id = prompt_id.0,
            passkey,
            entered,
            "bluetooth-agent: display passkey"
        );
    }

    async fn display_pin_code(
        &self,
        device: ObjectPath<'_>,
        pincode: &str,
    ) -> zbus::fdo::Result<()> {
        let device_path = device.as_str().to_owned();
        tracing::debug!(
            device = device_path,
            pin_length = pincode.chars().count(),
            prompt_kind = "display-pin",
            "bluetooth-agent: display pin requested"
        );
        let label = self.device_label(&device_path).await;
        let prompt_id = {
            let mut registry = self
                .registry
                .lock()
                .expect("bluetooth prompt registry poisoned");
            registry.publish_display_prompt(
                device_path.clone(),
                label,
                BluetoothPromptKind::DisplayPin {
                    pincode: pincode.to_owned(),
                },
            )
        }
        .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        tracing::info!(
            device = device_path,
            prompt_id = prompt_id.0,
            "bluetooth-agent: display pin code"
        );
        Ok(())
    }

    async fn request_pin_code(&self, device: ObjectPath<'_>) -> Result<String, BluezError> {
        let device_path = device.as_str().to_owned();
        tracing::debug!(
            device = device_path,
            prompt_kind = "request-pin",
            "bluetooth-agent: pin prompt requested"
        );
        self.request_string_reply(&device_path, BluetoothPromptKind::RequestPin, "pin")
            .await
    }

    fn cancel(&self) {
        tracing::debug!("bluetooth-agent: cancel requested by bluez");
        if self.cancel_prompt() {
            tracing::info!("bluetooth-agent: pairing cancelled");
        } else {
            tracing::info!("bluetooth-agent: pairing cancelled with no active prompt");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
