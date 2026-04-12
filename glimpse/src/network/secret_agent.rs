use std::collections::HashMap;

use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const AGENT_ID: &str = "me.aresa.glimpsed";
const AGENT_PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";

#[derive(Clone, Default)]
pub struct NetworkSecretAgent;

impl NetworkSecretAgent {
    pub async fn register(&self, conn: &zbus::Connection) -> zbus::Result<()> {
        let _ = conn.object_server().remove::<Self, _>(AGENT_PATH).await;
        conn.object_server().at(AGENT_PATH, self.clone()).await?;

        let agent_mgr = zbus::Proxy::new(
            conn,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager/AgentManager",
            "org.freedesktop.NetworkManager.AgentManager",
        )
        .await?;

        let _ = agent_mgr.call::<_, _, ()>("Unregister", &(AGENT_ID,)).await;

        agent_mgr
            .call::<_, _, ()>("Register", &(AGENT_ID,))
            .await
            .map_err(|error| {
                zbus::Error::Failure(format!("failed to register network secret agent: {error}"))
            })?;

        tracing::info!("network-secret-agent: registered as \"{AGENT_ID}\"");
        Ok(())
    }

    pub async fn unregister(&self, conn: &zbus::Connection) -> zbus::Result<()> {
        let agent_mgr = zbus::Proxy::new(
            conn,
            "org.freedesktop.NetworkManager",
            "/org/freedesktop/NetworkManager/AgentManager",
            "org.freedesktop.NetworkManager.AgentManager",
        )
        .await?;

        let _ = agent_mgr.call::<_, _, ()>("Unregister", &(AGENT_ID,)).await;
        let _ = conn.object_server().remove::<Self, _>(AGENT_PATH).await;
        tracing::info!("network-secret-agent: unregistered");
        Ok(())
    }
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl NetworkSecretAgent {
    async fn get_secrets(
        &self,
        connection: HashMap<String, HashMap<String, OwnedValue>>,
        connection_path: ObjectPath<'_>,
        setting_name: &str,
        _hints: Vec<String>,
        flags: u32,
    ) -> zbus::fdo::Result<HashMap<String, HashMap<String, OwnedValue>>> {
        let uuid = connection
            .get("connection")
            .and_then(|settings| settings.get("uuid"))
            .and_then(|value| String::try_from(value.clone()).ok())
            .unwrap_or_default();

        tracing::info!(
            uuid,
            setting_name,
            path = connection_path.as_str(),
            flags,
            "network-secret-agent: GetSecrets requested"
        );

        if uuid.is_empty() {
            tracing::warn!("network-secret-agent: no connection UUID, cannot look up secrets");
            return Ok(HashMap::new());
        }

        match lookup_keyring_secret(&uuid, setting_name).await {
            Ok(secrets) => {
                let count = secrets
                    .values()
                    .map(|settings| settings.len())
                    .sum::<usize>();
                tracing::info!(
                    count,
                    "network-secret-agent: returning secrets from keyring"
                );
                Ok(secrets)
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "network-secret-agent: keyring lookup failed"
                );
                Ok(HashMap::new())
            }
        }
    }

    async fn save_secrets(
        &self,
        _connection: HashMap<String, HashMap<String, OwnedValue>>,
        _connection_path: ObjectPath<'_>,
    ) -> zbus::fdo::Result<()> {
        Ok(())
    }

    async fn delete_secrets(
        &self,
        _connection: HashMap<String, HashMap<String, OwnedValue>>,
        _connection_path: ObjectPath<'_>,
    ) -> zbus::fdo::Result<()> {
        Ok(())
    }

    async fn cancel_get_secrets(
        &self,
        _connection_path: ObjectPath<'_>,
        _setting_name: &str,
    ) -> zbus::fdo::Result<()> {
        Ok(())
    }
}

pub(crate) fn secret_lookup_attributes<'a>(
    uuid: &'a str,
    setting_name: &'a str,
) -> HashMap<&'static str, &'a str> {
    HashMap::from([("connection-uuid", uuid), ("setting-name", setting_name)])
}

async fn lookup_keyring_secret(
    uuid: &str,
    setting_name: &str,
) -> anyhow::Result<HashMap<String, HashMap<String, OwnedValue>>> {
    let secret_service = secret_service::SecretService::connect(secret_service::EncryptionType::Dh)
        .await
        .map_err(|error| anyhow::anyhow!("failed to connect to Secret Service: {error}"))?;

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
        setting_secrets.insert(
            key.clone(),
            Value::from(secret)
                .try_to_owned()
                .map_err(|error| anyhow::anyhow!("value conversion: {error}"))?,
        );
    }

    if setting_secrets.is_empty() {
        return Ok(HashMap::new());
    }

    let mut result = HashMap::new();
    result.insert(setting_name.to_owned(), setting_secrets);
    Ok(result)
}
