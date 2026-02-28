use std::sync::Arc;

pub struct DbusProvider {
    pub connection: Arc<zbus::Connection>,
}

impl DbusProvider {
    pub fn connect() -> Self {
        let connection = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(zbus::Connection::session())
                .expect("failed to connect to d-bus session bus")
        });
        tracing::info!("connected to d-bus session bus");
        Self {
            connection: Arc::new(connection),
        }
    }
}
