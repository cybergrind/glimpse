mod app;
mod applets;
mod components;
mod compositors;
mod config;
mod dbus;
mod panels;
mod prompts;
mod services;
mod theme;

use relm4::{RELM_THREADS, RelmApp};
use tracing_subscriber::EnvFilter;

use crate::{
    app::{App, AppInit},
    compositors::detect_compositor,
    config::Config,
    dbus::Dbus,
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

    // must be set before relm4's runtime is first touched.
    let threads = std::env::var("GLIMPSE_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4);
    RELM_THREADS.set(threads).ok();

    let config = Config::autodetect();
    if let Some(compositor) = detect_compositor() {
        tracing::info!(compositor = compositor.name(), "detected compositor");
    } else {
        tracing::warn!("unsupported compositor");
    }

    let dbus = Dbus::connect()?;

    let app_id = "me.aresa.GlimpseShell";
    let app = RelmApp::new(app_id);

    register_resources();
    app.with_args(vec![]).run::<App>(AppInit { config, dbus });

    Ok(())
}
