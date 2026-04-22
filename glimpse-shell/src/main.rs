mod app;
mod config;
mod dbus;
mod panels;
mod services;
mod theme;

use relm4::RelmApp;
use tracing_subscriber::EnvFilter;

use crate::{
    app::{App, AppInit},
    config::Config,
    dbus::Dbus,
    services::framework::{Control, ServiceRuntime},
};
fn register_resources() {
    gio::resources_register_include!("glimpse-shell.gresource")
        .expect("failed to register embedded resources");
}

fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_env("GLIMPSE_LOG_LEVEL")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info,relm4=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let _rt_guard = rt.enter();

    let config = Config::autodetect();
    let dbus = rt.block_on(Dbus::connect())?;

    let services = ServiceRuntime::new(dbus);
    services.broadcast(Control::Start(config.clone()));

    let app_id = "me.aresa.GlimpseShell";
    let app = RelmApp::new(app_id);
    register_resources();
    theme::apply_theme(&config);
    app.with_args(vec![])
        .run::<App>(AppInit { config, services });

    Ok(())
}
