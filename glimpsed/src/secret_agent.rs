use std::collections::HashMap;

use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const AGENT_ID: &str = "me.aresa.glimpsed";
const AGENT_PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";

struct GlimpseSecretAgent;

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl GlimpseSecretAgent {
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
            .and_then(|c| c.get("uuid"))
            .and_then(|v| String::try_from(v.clone()).ok())
            .unwrap_or_default();

        tracing::info!(
            uuid,
            setting_name,
            path = connection_path.as_str(),
            flags,
            "secret-agent: GetSecrets requested"
        );

        if uuid.is_empty() {
            tracing::warn!("secret-agent: no connection UUID, cannot look up secrets");
            return Ok(HashMap::new());
        }

        match lookup_keyring_secret(&uuid, setting_name).await {
            Ok(secrets) => {
                let count = secrets.values().map(|m| m.len()).sum::<usize>();
                tracing::info!(count, "secret-agent: returning secrets from keyring");
                Ok(secrets)
            }
            Err(e) => {
                tracing::warn!(error = %e, "secret-agent: keyring lookup failed");
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

async fn lookup_keyring_secret(
    uuid: &str,
    setting_name: &str,
) -> anyhow::Result<HashMap<String, HashMap<String, OwnedValue>>> {
    let ss = secret_service::SecretService::connect(secret_service::EncryptionType::Dh).await
        .map_err(|e| anyhow::anyhow!("failed to connect to Secret Service: {e}"))?;

    // NM stores secrets with these attributes in gnome-keyring
    let attrs = HashMap::from([
        ("connection-uuid", uuid),
        ("setting-name", setting_name),
    ]);

    let results = ss.search_items(attrs).await
        .map_err(|e| anyhow::anyhow!("search failed: {e}"))?;

    let mut setting_secrets: HashMap<String, OwnedValue> = HashMap::new();

    // Check unlocked items first, then locked
    let items: Vec<_> = results.unlocked.into_iter().chain(results.locked).collect();

    for item in &items {
        let item_attrs = item.get_attributes().await
            .map_err(|e| anyhow::anyhow!("get_attributes: {e}"))?;

        let Some(key) = item_attrs.get("setting-key") else { continue };

        let secret_bytes = item.get_secret().await
            .map_err(|e| anyhow::anyhow!("get_secret: {e}"))?;

        let secret_str = String::from_utf8(secret_bytes)
            .map_err(|e| anyhow::anyhow!("secret is not UTF-8: {e}"))?;

        tracing::debug!(key, "secret-agent: found secret key");
        setting_secrets.insert(
            key.clone(),
            Value::from(secret_str).try_to_owned()
                .map_err(|e| anyhow::anyhow!("value conversion: {e}"))?,
        );
    }

    if setting_secrets.is_empty() {
        return Ok(HashMap::new());
    }

    let mut result = HashMap::new();
    result.insert(setting_name.to_owned(), setting_secrets);
    Ok(result)
}

pub async fn run(cancel: tokio_util::sync::CancellationToken) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;

    // Register our agent object on the bus
    conn.object_server()
        .at(AGENT_PATH, GlimpseSecretAgent)
        .await?;

    // Register with NM AgentManager
    let agent_mgr = zbus::Proxy::new(
        &conn,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager/AgentManager",
        "org.freedesktop.NetworkManager.AgentManager",
    )
    .await?;

    agent_mgr.call::<_, _, ()>("Register", &(AGENT_ID,)).await
        .map_err(|e| anyhow::anyhow!("failed to register secret agent: {e}"))?;

    tracing::info!("secret-agent: registered as \"{AGENT_ID}\"");

    cancel.cancelled().await;

    // Unregister on shutdown
    let _ = agent_mgr.call::<&str, _, ()>("Unregister", &(AGENT_ID,)).await;
    tracing::info!("secret-agent: unregistered");

    Ok(())
}
