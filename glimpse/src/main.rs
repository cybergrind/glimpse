mod app;
mod applets;
mod components;
mod panels;
mod providers;
use glimpse::config::Config;
use relm4::{RELM_THREADS, RelmApp};
use tracing_subscriber::EnvFilter;

use crate::app::App;

fn main() {
    let worker_threads = std::thread::available_parallelism()
        .map(|count| count.get().min(4).max(2))
        .unwrap_or(2);
    let _ = RELM_THREADS.set(worker_threads);

    let filter = EnvFilter::try_from_env("GLIMPSE_LOG_LEVEL")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info,relm4=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = Config::load();

    let app = RelmApp::new("me.aresa.GlimpsePanel");
    app.with_args(vec![]).run::<App>(config);
}
