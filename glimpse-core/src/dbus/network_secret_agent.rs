use zbus::zvariant::ObjectPath;

use crate::services::network::secret_agent::{ConnectionSettings, NetworkSecretAgent, SecretMap};

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl NetworkSecretAgent {
    async fn get_secrets(
        &self,
        connection: ConnectionSettings,
        connection_path: ObjectPath<'_>,
        setting_name: &str,
        _hints: Vec<String>,
        flags: u32,
    ) -> zbus::fdo::Result<SecretMap> {
        self.handle_get_secrets(connection, connection_path.as_str(), setting_name, flags)
            .await
    }

    async fn save_secrets(
        &self,
        connection: ConnectionSettings,
        connection_path: ObjectPath<'_>,
    ) -> zbus::fdo::Result<()> {
        self.handle_save_secrets(connection, connection_path.as_str())
            .await
    }

    async fn delete_secrets(
        &self,
        connection: ConnectionSettings,
        connection_path: ObjectPath<'_>,
    ) -> zbus::fdo::Result<()> {
        self.handle_delete_secrets(connection, connection_path.as_str())
            .await
    }

    async fn cancel_get_secrets(
        &self,
        connection_path: ObjectPath<'_>,
        setting_name: &str,
    ) -> zbus::fdo::Result<()> {
        self.handle_cancel_get_secrets(connection_path.as_str(), setting_name)
    }
}
