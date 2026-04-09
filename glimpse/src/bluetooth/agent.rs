use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

use tokio::sync::{oneshot, watch};
use zbus::zvariant::ObjectPath;

use crate::{
    bluetooth::protocol::{BluetoothPrompt, BluetoothPromptId, BluetoothPromptKind, BluetoothPromptReply},
    dbus::bluez::Device1Proxy,
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
    current_prompt: Option<BluetoothPrompt>,
    pending: HashMap<BluetoothPromptId, oneshot::Sender<BluetoothPromptReply>>,
    prompt_tx: watch::Sender<Option<BluetoothPrompt>>,
}

impl PromptRegistry {
    pub fn new(prompt_tx: watch::Sender<Option<BluetoothPrompt>>) -> Self {
        Self {
            next_prompt_id: 1,
            current_prompt: None,
            pending: HashMap::new(),
            prompt_tx,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<BluetoothPrompt>> {
        self.prompt_tx.subscribe()
    }

    pub fn current_prompt(&self) -> Option<BluetoothPrompt> {
        self.current_prompt.clone()
    }

    pub fn request_prompt(
        &mut self,
        device_path: String,
        device_label: String,
        kind: BluetoothPromptKind,
    ) -> Result<(BluetoothPromptId, oneshot::Receiver<BluetoothPromptReply>), PromptRegistryError>
    {
        if self.current_prompt.is_some() {
            return Err(PromptRegistryError::Busy);
        }

        let id = self.allocate_prompt_id();
        let (reply_tx, reply_rx) = oneshot::channel();

        self.pending.insert(id, reply_tx);
        self.publish_prompt(BluetoothPrompt {
            id,
            device_path,
            device_label,
            kind,
        });

        Ok((id, reply_rx))
    }

    pub fn publish_display_prompt(
        &mut self,
        device_path: String,
        device_label: String,
        kind: BluetoothPromptKind,
    ) -> BluetoothPromptId {
        let id = self.allocate_prompt_id();
        self.publish_prompt(BluetoothPrompt {
            id,
            device_path,
            device_label,
            kind,
        });
        id
    }

    pub fn complete(&mut self, id: BluetoothPromptId, reply: BluetoothPromptReply) -> bool {
        if self.current_prompt.as_ref().map(|prompt| prompt.id) != Some(id) {
            return false;
        }

        if let Some(reply_tx) = self.pending.remove(&id) {
            let _ = reply_tx.send(reply);
        }

        self.current_prompt = None;
        let _ = self.prompt_tx.send(None);
        true
    }

    pub fn cancel_current(&mut self) -> bool {
        let Some(prompt) = self.current_prompt.clone() else {
            return false;
        };

        let _ = self.pending.remove(&prompt.id).map(|reply_tx| {
            let _ = reply_tx.send(BluetoothPromptReply::Cancel);
        });
        self.current_prompt = None;
        let _ = self.prompt_tx.send(None);
        true
    }

    fn allocate_prompt_id(&mut self) -> BluetoothPromptId {
        let id = BluetoothPromptId(self.next_prompt_id);
        self.next_prompt_id += 1;
        id
    }

    fn publish_prompt(&mut self, prompt: BluetoothPrompt) {
        self.current_prompt = Some(prompt.clone());
        let _ = self.prompt_tx.send(Some(prompt));
    }
}

#[derive(Clone)]
pub struct BluetoothAgent {
    registry: Arc<Mutex<PromptRegistry>>,
    conn: zbus::Connection,
}

impl BluetoothAgent {
    pub fn new(registry: Arc<Mutex<PromptRegistry>>, conn: zbus::Connection) -> Self {
        Self { registry, conn }
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
            "bluetooth-agent: prompt emitted"
        );

        reply_rx
            .await
            .map_err(|_| BluezError::Rejected("bluetooth prompt reply channel closed".into()))
    }

    async fn request_string_reply(
        &self,
        device_path: &str,
        kind: BluetoothPromptKind,
        label: &'static str,
    ) -> Result<String, BluezError> {
        let reply = self.request_reply(device_path, kind).await?;
        match reply {
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
        let reply = self.request_reply(device_path, kind).await?;
        match reply {
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
        tracing::info!("bluetooth-agent: released");
    }

    async fn request_confirmation(
        &self,
        device: ObjectPath<'_>,
        passkey: u32,
    ) -> Result<(), BluezError> {
        let device_path = device.as_str().to_owned();
        tracing::info!(device = device_path, passkey, "bluetooth-agent: confirmation requested");
        let reply = self
            .request_reply(&device_path, BluetoothPromptKind::Confirm { passkey })
            .await?;
        match reply {
            BluetoothPromptReply::Confirm => Ok(()),
            BluetoothPromptReply::Cancel => Err(BluezError::Canceled("cancelled by user".into())),
            _ => Err(BluezError::Rejected("rejected by user".into())),
        }
    }

    async fn authorize_service(&self, device: ObjectPath<'_>, uuid: &str) -> zbus::fdo::Result<()> {
        tracing::info!(
            device = device.as_str(),
            uuid,
            "bluetooth-agent: authorizing service"
        );
        Ok(())
    }

    async fn request_authorization(&self, device: ObjectPath<'_>) -> zbus::fdo::Result<()> {
        tracing::info!(
            device = device.as_str(),
            "bluetooth-agent: authorizing pairing request"
        );
        Ok(())
    }

    async fn request_passkey(&self, device: ObjectPath<'_>) -> Result<u32, BluezError> {
        let device_path = device.as_str().to_owned();
        self.request_passkey_reply(&device_path, BluetoothPromptKind::RequestPasskey, "passkey")
            .await
    }

    async fn display_passkey(&self, device: ObjectPath<'_>, passkey: u32, entered: u16) {
        let device_path = device.as_str().to_owned();
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
        };
        tracing::info!(
            device = device_path,
            prompt_id = prompt_id.0,
            pincode,
            "bluetooth-agent: display pin code"
        );
        Ok(())
    }

    async fn request_pin_code(&self, device: ObjectPath<'_>) -> Result<String, BluezError> {
        let device_path = device.as_str().to_owned();
        self.request_string_reply(&device_path, BluetoothPromptKind::RequestPin, "pin")
            .await
    }

    fn cancel(&self) {
        let mut registry = self
            .registry
            .lock()
            .expect("bluetooth prompt registry poisoned");
        if registry.cancel_current() {
            tracing::info!("bluetooth-agent: pairing cancelled");
        } else {
            tracing::info!("bluetooth-agent: pairing cancelled with no active prompt");
        }
    }
}

impl BluetoothAgent {
    pub async fn register(&self, conn: &zbus::Connection) -> zbus::Result<()> {
        let _ = conn.object_server().remove::<Self, _>(AGENT_PATH).await;
        conn.object_server().at(AGENT_PATH, self.clone()).await?;

        let agent_mgr =
            zbus::Proxy::new(conn, "org.bluez", "/org/bluez", "org.bluez.AgentManager1").await?;
        let path = ObjectPath::try_from(AGENT_PATH)?;

        let _ = agent_mgr
            .call::<_, _, ()>("UnregisterAgent", &(&path,))
            .await;

        agent_mgr
            .call::<_, _, ()>("RegisterAgent", &(&path, "KeyboardDisplay"))
            .await
            .map_err(|error| {
                zbus::Error::Failure(format!("failed to register bluetooth agent: {error}"))
            })?;

        let _ = agent_mgr
            .call::<_, _, ()>("RequestDefaultAgent", &(&path,))
            .await;

        tracing::info!("bluetooth-agent: registered");
        Ok(())
    }

    pub async fn unregister(&self, conn: &zbus::Connection) -> zbus::Result<()> {
        let agent_mgr =
            zbus::Proxy::new(conn, "org.bluez", "/org/bluez", "org.bluez.AgentManager1").await?;
        let path = ObjectPath::try_from(AGENT_PATH)?;

        let _ = agent_mgr
            .call::<_, _, ()>("UnregisterAgent", &(&path,))
            .await;
        let _ = conn.object_server().remove::<Self, _>(AGENT_PATH).await;
        tracing::info!("bluetooth-agent: unregistered");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::watch;

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
        let emitted_prompt = prompt_rx.borrow().clone().expect("prompt emission");
        assert_eq!(emitted_prompt.id, prompt_id);

        assert!(registry.complete(prompt_id, BluetoothPromptReply::Pin("1234".into()),));

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

        let prompt_id = registry.publish_display_prompt(
            "/org/bluez/hci0/dev_AA_BB".into(),
            "Headphones".into(),
            BluetoothPromptKind::DisplayPasskey {
                passkey: 123_456,
                entered: 3,
            },
        );

        assert_eq!(
            prompt_rx.borrow().as_ref().map(|prompt| prompt.kind.clone()),
            Some(BluetoothPromptKind::DisplayPasskey {
                passkey: 123_456,
                entered: 3,
            })
        );
        assert!(registry.complete(prompt_id, BluetoothPromptReply::Cancel));
        assert!(prompt_rx.borrow().is_none());
    }

    #[test]
    fn cancel_and_reject_keep_distinct_bluez_errors() {
        assert_eq!(
            BluezError::Canceled("cancelled".into()).to_string(),
            "org.bluez.Error.Canceled: cancelled"
        );
        assert_eq!(
            BluezError::Rejected("rejected".into()).to_string(),
            "org.bluez.Error.Rejected: rejected"
        );
    }
}
