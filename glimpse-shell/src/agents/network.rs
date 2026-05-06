use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::{oneshot, watch};
use tokio::time::{Duration, sleep};
use tokio_util::sync::CancellationToken;
use zbus::zvariant::{OwnedValue, Value};

const AGENT_PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";
const WIFI_SECURITY_SETTING: &str = "802-11-wireless-security";
const WIFI_PSK_KEY: &str = "psk";
const SECRET_CONTENT_TYPE: &str = "text/plain";
const NM_SECRET_AGENT_GET_SECRETS_FLAG_NONE: u32 = 0x0;
const NM_SECRET_AGENT_GET_SECRETS_FLAG_ALLOW_INTERACTION: u32 = 0x1;
const NM_SECRET_AGENT_GET_SECRETS_FLAG_REQUEST_NEW: u32 = 0x2;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct NetworkPromptId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkPrompt {
    pub id: NetworkPromptId,
    pub ssid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkPromptReply {
    Password(String),
    Cancel,
}

pub(crate) type ConnectionSettings = HashMap<String, HashMap<String, OwnedValue>>;
pub(crate) type SecretMap = HashMap<String, HashMap<String, OwnedValue>>;

struct PendingPrompt {
    prompt: NetworkPrompt,
    reply_tx: oneshot::Sender<NetworkPromptReply>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyringSecretEntry {
    uuid: String,
    connection_label: String,
    setting_name: &'static str,
    setting_key: &'static str,
    secret: String,
}

struct PromptRegistry {
    next_prompt_id: u64,
    current: Option<PendingPrompt>,
    prompt_tx: watch::Sender<Option<NetworkPrompt>>,
}

impl PromptRegistry {
    fn new(prompt_tx: watch::Sender<Option<NetworkPrompt>>) -> Self {
        Self {
            next_prompt_id: 1,
            current: None,
            prompt_tx,
        }
    }

    fn request_prompt(
        &mut self,
        ssid: String,
    ) -> Option<(NetworkPromptId, oneshot::Receiver<NetworkPromptReply>)> {
        if self.current.is_some() {
            tracing::debug!(
                ssid,
                "network-secret-agent: prompt request rejected because another prompt is active"
            );
            return None;
        }

        let id = self.allocate_prompt_id();
        let (reply_tx, reply_rx) = oneshot::channel();
        let prompt = NetworkPrompt { id, ssid };

        tracing::debug!(
            prompt_id = id.0,
            ssid = prompt.ssid,
            "network-secret-agent: prompt request registered"
        );
        self.current = Some(PendingPrompt {
            prompt: prompt.clone(),
            reply_tx,
        });
        let _ = self.prompt_tx.send(Some(prompt));

        Some((id, reply_rx))
    }

    fn complete(&mut self, id: NetworkPromptId, reply: NetworkPromptReply) -> bool {
        if self.current.as_ref().map(|current| current.prompt.id) != Some(id) {
            tracing::debug!(
                prompt_id = id.0,
                reply = prompt_reply_name(&reply),
                "network-secret-agent: prompt reply ignored because prompt is not active"
            );
            return false;
        }

        let current = self.current.take();
        if let Some(current) = current {
            tracing::debug!(
                prompt_id = id.0,
                ssid = current.prompt.ssid,
                reply = prompt_reply_name(&reply),
                "network-secret-agent: prompt completed"
            );
            let _ = current.reply_tx.send(reply);
        }

        let _ = self.prompt_tx.send(None);
        true
    }

    fn cancel_current(&mut self) -> bool {
        let Some(current) = self.current.take() else {
            tracing::debug!("network-secret-agent: cancel ignored because no prompt is active");
            return false;
        };

        tracing::debug!(
            prompt_id = current.prompt.id.0,
            ssid = current.prompt.ssid,
            "network-secret-agent: active prompt cancelled"
        );
        let _ = current.reply_tx.send(NetworkPromptReply::Cancel);

        let _ = self.prompt_tx.send(None);
        true
    }

    fn allocate_prompt_id(&mut self) -> NetworkPromptId {
        let id = NetworkPromptId(self.next_prompt_id);
        self.next_prompt_id += 1;
        id
    }
}

fn prompt_reply_name(reply: &NetworkPromptReply) -> &'static str {
    match reply {
        NetworkPromptReply::Password(_) => "password",
        NetworkPromptReply::Cancel => "cancel",
    }
}

#[derive(Clone)]
pub struct NetworkSecretAgent {
    registry: Arc<Mutex<PromptRegistry>>,
    conn: zbus::Connection,
}

impl NetworkSecretAgent {
    pub fn new(conn: zbus::Connection) -> (Self, watch::Receiver<Option<NetworkPrompt>>) {
        let (prompt_tx, prompt_rx) = watch::channel(None);
        let registry = Arc::new(Mutex::new(PromptRegistry::new(prompt_tx)));

        (Self { registry, conn }, prompt_rx)
    }

    pub fn complete_prompt(&self, id: NetworkPromptId, reply: NetworkPromptReply) -> bool {
        self.registry
            .lock()
            .expect("network prompt registry poisoned")
            .complete(id, reply)
    }

    pub fn cancel_prompt(&self) -> bool {
        self.registry
            .lock()
            .expect("network prompt registry poisoned")
            .cancel_current()
    }

    pub async fn register(&self) -> zbus::Result<()> {
        let agent_id = network_secret_agent_id();
        tracing::debug!(
            path = AGENT_PATH,
            agent_id,
            "network-secret-agent: registering object"
        );

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
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager/AgentManager",
            "org.freedesktop.NetworkManager.AgentManager",
        )
        .await?;

        tracing::debug!(agent_id, "network-secret-agent: unregistering stale agent");
        let _ = agent_mgr
            .call::<_, _, ()>("Unregister", &(agent_id.as_str(),))
            .await;

        tracing::debug!(
            agent_id,
            "network-secret-agent: registering with NetworkManager"
        );
        agent_mgr
            .call::<_, _, ()>("Register", &(agent_id.as_str(),))
            .await
            .map_err(|error| {
                zbus::Error::Failure(format!("failed to register network secret agent: {error}"))
            })?;

        tracing::info!(agent_id, "network-secret-agent: registered");
        Ok(())
    }

    pub async fn unregister(&self) -> zbus::Result<()> {
        let agent_id = network_secret_agent_id();
        tracing::debug!(agent_id, "network-secret-agent: unregistering");

        let agent_mgr = zbus::Proxy::new(
            &self.conn,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager/AgentManager",
            "org.freedesktop.NetworkManager.AgentManager",
        )
        .await?;

        let _ = agent_mgr
            .call::<_, _, ()>("Unregister", &(agent_id.as_str(),))
            .await;
        let _ = self
            .conn
            .object_server()
            .remove::<Self, _>(AGENT_PATH)
            .await;
        self.cancel_prompt();

        tracing::info!("network-secret-agent: unregistered");
        Ok(())
    }

    async fn request_password(&self, ssid: String) -> zbus::fdo::Result<String> {
        let (id, reply_rx) = {
            let mut registry = self
                .registry
                .lock()
                .expect("network prompt registry poisoned");
            registry.request_prompt(ssid.clone()).ok_or_else(|| {
                zbus::fdo::Error::Failed("a network prompt is already active".into())
            })?
        };

        tracing::info!(
            prompt_id = id.0,
            ssid,
            "network-secret-agent: password prompt emitted"
        );

        match reply_rx.await {
            Ok(NetworkPromptReply::Password(password)) => {
                tracing::debug!(
                    prompt_id = id.0,
                    "network-secret-agent: password reply received"
                );
                Ok(password)
            }
            Ok(NetworkPromptReply::Cancel) => {
                tracing::debug!(
                    prompt_id = id.0,
                    "network-secret-agent: password prompt cancelled"
                );
                Err(zbus::fdo::Error::Failed("cancelled by user".into()))
            }
            Err(_) => Err(zbus::fdo::Error::Failed(
                "network prompt reply channel closed".into(),
            )),
        }
    }

    pub(crate) async fn handle_get_secrets(
        &self,
        connection: ConnectionSettings,
        connection_path: &str,
        setting_name: &str,
        flags: u32,
    ) -> zbus::fdo::Result<SecretMap> {
        let uuid = string_setting(&connection, "connection", "uuid").unwrap_or_default();
        let label = connection_label(&connection, connection_path);

        tracing::debug!(
            uuid,
            setting_name,
            path = connection_path,
            flags,
            "network-secret-agent: GetSecrets requested"
        );

        if !uuid.is_empty() {
            match lookup_keyring_secret(&uuid, setting_name).await {
                Ok(secrets) if !secrets.is_empty() => {
                    let count = secrets
                        .values()
                        .map(|settings| settings.len())
                        .sum::<usize>();
                    tracing::debug!(
                        count,
                        "network-secret-agent: returning secrets from keyring"
                    );
                    return Ok(secrets);
                }
                Ok(_) => {
                    tracing::debug!("network-secret-agent: no keyring secrets found");
                }
                Err(error) => {
                    tracing::warn!(error = %error, "network-secret-agent: keyring lookup failed");
                }
            }
        } else {
            tracing::debug!("network-secret-agent: no connection UUID for keyring lookup");
        }

        if setting_name != WIFI_SECURITY_SETTING {
            tracing::debug!(
                setting_name,
                "network-secret-agent: unsupported interactive secret setting"
            );
            return Ok(HashMap::new());
        }

        if !secrets_request_allows_interaction(flags) {
            tracing::debug!("network-secret-agent: interaction not allowed for GetSecrets request");
            return Ok(HashMap::new());
        }

        let password = self.request_password(label).await?;
        Ok(wifi_password_secret_map(&password))
    }

    pub(crate) async fn handle_save_secrets(
        &self,
        connection: ConnectionSettings,
        connection_path: &str,
    ) -> zbus::fdo::Result<()> {
        tracing::debug!(
            uuid = string_setting(&connection, "connection", "uuid").unwrap_or_default(),
            path = connection_path,
            "network-secret-agent: SaveSecrets requested"
        );
        save_keyring_secrets(&connection)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(format!("failed to save secrets: {error}")))
    }

    pub(crate) async fn handle_delete_secrets(
        &self,
        connection: ConnectionSettings,
        connection_path: &str,
    ) -> zbus::fdo::Result<()> {
        tracing::debug!(
            uuid = string_setting(&connection, "connection", "uuid").unwrap_or_default(),
            path = connection_path,
            "network-secret-agent: DeleteSecrets requested"
        );
        delete_keyring_secrets(&connection)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(format!("failed to delete secrets: {error}")))
    }

    pub(crate) fn handle_cancel_get_secrets(
        &self,
        connection_path: &str,
        setting_name: &str,
    ) -> zbus::fdo::Result<()> {
        tracing::debug!(
            path = connection_path,
            setting_name,
            "network-secret-agent: CancelGetSecrets requested"
        );
        self.cancel_prompt();
        Ok(())
    }
}

#[derive(Clone)]
pub struct NetworkAgentHandle {
    agent: NetworkSecretAgent,
    prompt_rx: watch::Receiver<Option<NetworkPrompt>>,
}

impl NetworkAgentHandle {
    pub fn subscribe(&self) -> watch::Receiver<Option<NetworkPrompt>> {
        self.prompt_rx.clone()
    }

    pub fn reply(&self, id: NetworkPromptId, reply: NetworkPromptReply) -> bool {
        self.agent.complete_prompt(id, reply)
    }
}

pub struct NetworkAgentRuntime {
    agent: NetworkSecretAgent,
}

impl NetworkAgentRuntime {
    pub fn new(conn: zbus::Connection) -> (Self, NetworkAgentHandle) {
        let (agent, prompt_rx) = NetworkSecretAgent::new(conn);
        let handle = NetworkAgentHandle {
            agent: agent.clone(),
            prompt_rx,
        };

        (Self { agent }, handle)
    }

    pub async fn run(self, cancel: CancellationToken) {
        loop {
            match self.agent.register().await {
                Ok(()) => {
                    cancel.cancelled().await;
                    if let Err(error) = self.agent.unregister().await {
                        tracing::warn!(error = %error, "network-secret-agent: unregister failed");
                    }
                    break;
                }
                Err(error) => {
                    tracing::warn!(error = %error, "network-secret-agent: register failed, retrying");
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = sleep(Duration::from_secs(2)) => {}
                    }
                }
            }
        }
    }
}

fn network_secret_agent_id() -> String {
    format!("me.aresa.glimpse.network-agent.{}", std::process::id())
}

pub(crate) fn secret_lookup_attributes<'a>(
    uuid: &'a str,
    setting_name: &'a str,
) -> HashMap<&'static str, &'a str> {
    HashMap::from([("connection-uuid", uuid), ("setting-name", setting_name)])
}

fn secret_item_attributes<'a>(
    uuid: &'a str,
    setting_name: &'a str,
    setting_key: &'a str,
) -> HashMap<&'static str, &'a str> {
    HashMap::from([
        ("connection-uuid", uuid),
        ("setting-name", setting_name),
        ("setting-key", setting_key),
    ])
}

fn connection_label(connection: &ConnectionSettings, fallback: &str) -> String {
    ssid_setting(connection, "802-11-wireless", "ssid")
        .filter(|ssid| !ssid.is_empty())
        .or_else(|| string_setting(connection, "connection", "id").filter(|id| !id.is_empty()))
        .unwrap_or_else(|| fallback.to_owned())
}

fn secrets_request_allows_interaction(flags: u32) -> bool {
    flags
        & (NM_SECRET_AGENT_GET_SECRETS_FLAG_ALLOW_INTERACTION
            | NM_SECRET_AGENT_GET_SECRETS_FLAG_REQUEST_NEW)
        != NM_SECRET_AGENT_GET_SECRETS_FLAG_NONE
}

fn ssid_setting(connection: &ConnectionSettings, setting_name: &str, key: &str) -> Option<String> {
    connection
        .get(setting_name)
        .and_then(|setting| setting.get(key))
        .and_then(|value| Vec::<u8>::try_from(value.clone()).ok())
        .map(|ssid| String::from_utf8_lossy(&ssid).trim_matches('\0').to_owned())
}

fn string_setting(
    connection: &ConnectionSettings,
    setting_name: &str,
    key: &str,
) -> Option<String> {
    connection
        .get(setting_name)
        .and_then(|setting| setting.get(key))
        .and_then(|value| String::try_from(value.clone()).ok())
}

fn wifi_password_secret_map(password: &str) -> SecretMap {
    HashMap::from([(
        WIFI_SECURITY_SETTING.to_owned(),
        HashMap::from([(WIFI_PSK_KEY.to_owned(), owned_string(password))]),
    )])
}

fn owned_string(value: &str) -> OwnedValue {
    Value::from(value)
        .try_to_owned()
        .expect("string value should convert to owned value")
}

#[cfg(test)]
fn owned_bytes(value: &[u8]) -> OwnedValue {
    Value::from(value.to_vec())
        .try_to_owned()
        .expect("byte array value should convert to owned value")
}

fn keyring_secret_entries(connection: &ConnectionSettings) -> Vec<KeyringSecretEntry> {
    let Some(uuid) =
        string_setting(connection, "connection", "uuid").filter(|uuid| !uuid.is_empty())
    else {
        return Vec::new();
    };

    let connection_label = connection_label(connection, &uuid);
    let mut entries = Vec::new();

    if let Some(secret) = string_setting(connection, WIFI_SECURITY_SETTING, WIFI_PSK_KEY)
        .filter(|psk| !psk.is_empty())
    {
        entries.push(KeyringSecretEntry {
            uuid,
            connection_label,
            setting_name: WIFI_SECURITY_SETTING,
            setting_key: WIFI_PSK_KEY,
            secret,
        });
    }

    entries
}

fn keyring_secret_label(entry: &KeyringSecretEntry) -> String {
    format!(
        "Network secret for {}: {}.{}",
        entry.connection_label, entry.setting_name, entry.setting_key
    )
}

async fn connect_secret_service() -> anyhow::Result<secret_service::SecretService<'static>> {
    secret_service::SecretService::connect(secret_service::EncryptionType::Dh)
        .await
        .map_err(|error| anyhow::anyhow!("failed to connect to Secret Service: {error}"))
}

async fn save_keyring_secrets(connection: &ConnectionSettings) -> anyhow::Result<()> {
    let entries = keyring_secret_entries(connection);
    if entries.is_empty() {
        tracing::debug!("network-secret-agent: no supported secrets to save");
        return Ok(());
    }

    let secret_service = connect_secret_service().await?;
    let collection = secret_service
        .get_default_collection()
        .await
        .map_err(|error| anyhow::anyhow!("failed to open default collection: {error}"))?;

    for entry in entries {
        let attributes = secret_item_attributes(&entry.uuid, entry.setting_name, entry.setting_key);
        collection
            .create_item(
                &keyring_secret_label(&entry),
                attributes,
                entry.secret.as_bytes(),
                true,
                SECRET_CONTENT_TYPE,
            )
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "failed to save {}.{}: {error}",
                    entry.setting_name,
                    entry.setting_key
                )
            })?;

        tracing::debug!(
            uuid = entry.uuid,
            setting_name = entry.setting_name,
            setting_key = entry.setting_key,
            "network-secret-agent: saved secret to keyring"
        );
    }

    Ok(())
}

async fn delete_keyring_secrets(connection: &ConnectionSettings) -> anyhow::Result<()> {
    let Some(uuid) =
        string_setting(connection, "connection", "uuid").filter(|uuid| !uuid.is_empty())
    else {
        tracing::debug!("network-secret-agent: no connection UUID for keyring delete");
        return Ok(());
    };

    let secret_service = connect_secret_service().await?;
    let results = secret_service
        .search_items(HashMap::from([("connection-uuid", uuid.as_str())]))
        .await
        .map_err(|error| anyhow::anyhow!("search failed: {error}"))?;
    let items: Vec<_> = results.unlocked.into_iter().chain(results.locked).collect();
    let count = items.len();

    for item in items {
        item.delete()
            .await
            .map_err(|error| anyhow::anyhow!("delete failed: {error}"))?;
    }

    tracing::debug!(uuid, count, "network-secret-agent: deleted keyring secrets");
    Ok(())
}

async fn lookup_keyring_secret(uuid: &str, setting_name: &str) -> anyhow::Result<SecretMap> {
    let secret_service = connect_secret_service().await?;

    let results = secret_service
        .search_items(secret_lookup_attributes(uuid, setting_name))
        .await
        .map_err(|error| anyhow::anyhow!("search failed: {error}"))?;

    let mut setting_secrets = HashMap::new();
    let items: Vec<_> = results.unlocked.into_iter().chain(results.locked).collect();

    for item in &items {
        let item_attrs = item
            .get_attributes()
            .await
            .map_err(|error| anyhow::anyhow!("get_attributes: {error}"))?;

        let Some(key) = item_attrs.get("setting-key") else {
            continue;
        };

        let secret_bytes = item
            .get_secret()
            .await
            .map_err(|error| anyhow::anyhow!("get_secret: {error}"))?;

        let secret = String::from_utf8(secret_bytes)
            .map_err(|error| anyhow::anyhow!("secret is not UTF-8: {error}"))?;

        tracing::debug!(key, "network-secret-agent: found secret key");
        setting_secrets.insert(key.clone(), owned_string(&secret));
    }

    if setting_secrets.is_empty() {
        return Ok(HashMap::new());
    }

    Ok(HashMap::from([(setting_name.to_owned(), setting_secrets)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tokio::sync::watch;

    #[test]
    fn secret_lookup_attributes_match_network_manager_keyring_schema() {
        let attrs = secret_lookup_attributes("uuid-1", "802-11-wireless-security");

        assert_eq!(attrs.get("connection-uuid"), Some(&"uuid-1"));
        assert_eq!(attrs.get("setting-name"), Some(&"802-11-wireless-security"));
    }

    #[test]
    fn secret_item_attributes_include_setting_key() {
        let attrs = secret_item_attributes("uuid-1", "802-11-wireless-security", "psk");

        assert_eq!(attrs.get("connection-uuid"), Some(&"uuid-1"));
        assert_eq!(attrs.get("setting-name"), Some(&"802-11-wireless-security"));
        assert_eq!(attrs.get("setting-key"), Some(&"psk"));
    }

    #[test]
    fn wifi_password_secret_map_uses_wireless_security_psk_key() {
        let secrets = wifi_password_secret_map("secret");
        let security = secrets
            .get("802-11-wireless-security")
            .expect("wireless security setting");
        let psk = security.get("psk").expect("psk secret");

        assert_eq!(String::try_from(psk.clone()).unwrap(), "secret");
    }

    #[test]
    fn connection_label_prefers_connection_id() {
        let mut connection = HashMap::new();
        connection.insert(
            "connection".into(),
            HashMap::from([("id".into(), owned_string("Office Wi-Fi"))]),
        );

        assert_eq!(
            connection_label(&connection, "/org/freedesktop/NetworkManager/Settings/1"),
            "Office Wi-Fi"
        );
    }

    #[test]
    fn connection_label_prefers_wireless_ssid_over_connection_id() {
        let mut connection = HashMap::new();
        connection.insert(
            "connection".into(),
            HashMap::from([("id".into(), owned_string("Auto eth0"))]),
        );
        connection.insert(
            "802-11-wireless".into(),
            HashMap::from([("ssid".into(), owned_bytes(b"Office Wi-Fi"))]),
        );

        assert_eq!(
            connection_label(&connection, "/org/freedesktop/NetworkManager/Settings/1"),
            "Office Wi-Fi"
        );
    }

    #[test]
    fn request_new_flag_allows_interaction() {
        assert!(!secrets_request_allows_interaction(
            NM_SECRET_AGENT_GET_SECRETS_FLAG_NONE
        ));
        assert!(secrets_request_allows_interaction(
            NM_SECRET_AGENT_GET_SECRETS_FLAG_ALLOW_INTERACTION
        ));
        assert!(secrets_request_allows_interaction(
            NM_SECRET_AGENT_GET_SECRETS_FLAG_REQUEST_NEW
        ));
    }

    #[test]
    fn keyring_secret_entries_extract_supported_wifi_psk() {
        let mut connection = HashMap::new();
        connection.insert(
            "connection".into(),
            HashMap::from([
                ("uuid".into(), owned_string("uuid-1")),
                ("id".into(), owned_string("Office")),
            ]),
        );
        connection.insert(
            "802-11-wireless-security".into(),
            HashMap::from([("psk".into(), owned_string("secret"))]),
        );

        assert_eq!(
            keyring_secret_entries(&connection),
            vec![KeyringSecretEntry {
                uuid: "uuid-1".into(),
                connection_label: "Office".into(),
                setting_name: "802-11-wireless-security",
                setting_key: "psk",
                secret: "secret".into(),
            }]
        );
    }

    #[test]
    fn keyring_secret_entries_ignore_missing_uuid_or_secret() {
        assert!(keyring_secret_entries(&HashMap::new()).is_empty());

        let mut connection = HashMap::new();
        connection.insert(
            "connection".into(),
            HashMap::from([("uuid".into(), owned_string("uuid-1"))]),
        );

        assert!(keyring_secret_entries(&connection).is_empty());
    }

    #[tokio::test]
    async fn prompt_registry_completes_matching_prompt() {
        let (prompt_tx, prompt_rx) = watch::channel(None);
        let mut registry = PromptRegistry::new(prompt_tx);
        let (id, reply_rx) = registry
            .request_prompt("Office".into())
            .expect("prompt should be accepted");

        assert_eq!(prompt_rx.borrow().as_ref().unwrap().ssid, "Office");
        assert!(registry.complete(id, NetworkPromptReply::Password("secret".into())));
        assert_eq!(
            reply_rx.await.unwrap(),
            NetworkPromptReply::Password("secret".into())
        );
        assert!(prompt_rx.borrow().is_none());
    }

    #[test]
    fn prompt_registry_rejects_concurrent_prompt() {
        let (prompt_tx, _prompt_rx) = watch::channel(None);
        let mut registry = PromptRegistry::new(prompt_tx);

        registry
            .request_prompt("Office".into())
            .expect("first prompt should be accepted");

        assert!(registry.request_prompt("Guest".into()).is_none());
    }
}
