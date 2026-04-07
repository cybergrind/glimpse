pub struct DbusProvider {
    pub session: zbus::Connection,
    pub system: zbus::Connection,
}

impl DbusProvider {
    pub fn connect() -> Self {
        let (session, system) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let session = zbus::Connection::session()
                    .await
                    .expect("failed to connect to d-bus session bus");
                let system = zbus::Connection::system()
                    .await
                    .expect("failed to connect to d-bus system bus");
                (session, system)
            })
        });
        tracing::info!("connected to d-bus session and system buses");
        Self { session, system }
    }
}
