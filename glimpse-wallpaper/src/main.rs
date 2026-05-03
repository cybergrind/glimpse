use glimpse_config::Config;
use glimpse_wallpaper::{
    app::{AppInit, WallpaperAppModel},
    runtime::{GTK_APPLICATION_ID, WallpaperRuntime},
};
use relm4::{RELM_THREADS, RelmApp};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_env("GLIMPSE_LOG_LEVEL")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info,relm4=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let threads = std::env::var("GLIMPSE_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4);
    RELM_THREADS.set(threads).ok();

    let _single_instance = match WallpaperRuntime::acquire_single_instance().await {
        Ok(guard) => guard,
        Err(err) => {
            tracing::error!("failed to start glimpse-wallpaper: {err}");
            return Err(err);
        }
    };

    let config = Config::load();
    let app = RelmApp::new(GTK_APPLICATION_ID);
    app.with_args(vec![])
        .run::<WallpaperAppModel>(AppInit { config });

    Ok(())
}
