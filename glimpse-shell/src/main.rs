mod app;
mod config;
mod dbus;
mod services;

use relm4::RelmApp;
use tracing_subscriber::EnvFilter;

use crate::{
    app::{App, AppInit},
    config::Config,
    dbus::Dbus,
    services::framework::{Control, Services},
};

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_env("GLIMPSE_LOG_LEVEL")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info,relm4=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = Config::autodetect();
    let dbus = Dbus::connect();
    let system_dbus = dbus.system.clone();
    let session_dbus = dbus.session.clone();
    let services = Services::new(session_dbus, system_dbus);
    services.broadcast(Control::Start(config.clone()));

    let app_id = "me.aresa.GlimpseShell";
    let app = RelmApp::new(app_id);
    app.with_args(vec![])
        .run::<App>(AppInit { config, services });
}
