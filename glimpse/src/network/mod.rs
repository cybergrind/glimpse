pub mod protocol;
pub mod provider;
mod secret_agent;
pub mod service;

pub use service::NetworkServiceHandle;

#[cfg(test)]
mod tests {
    use super::secret_agent::secret_lookup_attributes;

    #[test]
    fn networkmanager_secret_agent_uses_expected_keyring_attributes() {
        let attrs = secret_lookup_attributes("uuid-123", "802-11-wireless-security");

        assert_eq!(attrs.get("connection-uuid"), Some(&"uuid-123"));
        assert_eq!(attrs.get("setting-name"), Some(&"802-11-wireless-security"));
    }
}
