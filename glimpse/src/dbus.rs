use futures_util::StreamExt;
use zbus::zvariant::OwnedValue;

/// Helper for D-Bus providers that follow the common pattern:
/// create proxy → read properties → stream PropertiesChanged → re-read.
pub struct DbusPropertyGroup {
    proxy: zbus::Proxy<'static>,
}

impl DbusPropertyGroup {
    pub async fn new(
        conn: &zbus::Connection,
        service: &str,
        path: &str,
        interface: &str,
    ) -> zbus::Result<Self> {
        let proxy = zbus::Proxy::new(
            conn,
            service.to_owned(),
            zbus::zvariant::ObjectPath::try_from(path.to_owned())?,
            zbus::names::InterfaceName::try_from(interface.to_owned())?,
        )
        .await?;
        Ok(Self { proxy })
    }

    /// Read a property via an explicit D-Bus Properties.Get call (never cached).
    pub async fn get<T: TryFrom<OwnedValue>>(&self, name: &str) -> Option<T> {
        let conn = self.proxy.connection();
        let props = zbus::fdo::PropertiesProxy::builder(conn)
            .destination(self.proxy.destination().to_owned())
            .ok()?
            .path(self.proxy.path().to_owned())
            .ok()?
            .build()
            .await
            .ok()?;
        let value = props
            .get(self.proxy.interface().to_owned(), name)
            .await
            .ok()?;
        T::try_from(value).ok()
    }

    /// Set a property.
    pub async fn set<T: Into<zbus::zvariant::Value<'static>> + Send + Sync + 'static>(
        &self,
        name: &str,
        value: T,
    ) -> zbus::fdo::Result<()> {
        self.proxy.set_property(name, value).await
    }

    /// Call a method with args and return the result.
    pub async fn call<B, R>(&self, method: &str, args: &B) -> zbus::Result<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType + Sync,
        R: serde::de::DeserializeOwned + zbus::zvariant::Type,
    {
        self.proxy.call(method, args).await
    }

    /// Call a method with no return value.
    pub async fn call_void<B>(&self, method: &str, args: &B) -> zbus::Result<()>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType + Sync,
    {
        self.proxy.call::<_, _, ()>(method, args).await
    }

    /// Stream property change notifications. Yields changed property names on each signal.
    pub async fn stream_changes(
        &self,
    ) -> zbus::Result<impl futures_util::Stream<Item = Vec<String>>> {
        let conn = self.proxy.connection();
        let props = zbus::fdo::PropertiesProxy::builder(conn)
            .destination(self.proxy.destination().to_owned())?
            .path(self.proxy.path().to_owned())?
            .build()
            .await?;
        let stream = props.receive_properties_changed().await?;
        Ok(stream.map(|signal| {
            signal
                .args()
                .ok()
                .map(|args| {
                    args.changed_properties()
                        .keys()
                        .map(|k| k.to_string())
                        .collect()
                })
                .unwrap_or_default()
        }))
    }
}
