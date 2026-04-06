mod app;
mod applets;
mod config;
mod panels;
mod providers;
use config::Config;
use relm4::RelmApp;
use tracing_subscriber::EnvFilter;

use crate::app::App;

fn main() {
    let filter =
        EnvFilter::try_from_env("GLIMPSE_LOG_LEVEL").unwrap_or_else(|_| EnvFilter::new("info,relm4=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = Config::load();

    let app = RelmApp::new("me.aresa.GlimpsePanel");
    app.with_args(vec![]).run::<App>(config);
}
