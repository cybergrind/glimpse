use glimpse_config::Config;
use glimpse_wallpaper::{
    app::{AppInit, WallpaperAppModel},
    runtime::{GTK_APPLICATION_ID, WallpaperRuntime},
};
use relm4::{
    RELM_THREADS, RelmApp,
    gtk::{self, gio::prelude::ApplicationExtManual},
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = log_filter();
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let threads = std::env::var("GLIMPSE_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4);
    RELM_THREADS.set(threads).ok();
    tracing::debug!(threads, "configured Relm4 worker threads");

    let _single_instance = match WallpaperRuntime::acquire_single_instance().await {
        Ok(guard) => {
            tracing::info!("acquired single-instance D-Bus name");
            guard
        }
        Err(err) => {
            tracing::error!("failed to start glimpse-wallpaper: {err}");
            return Err(err);
        }
    };

    let config = Config::load();
    tracing::debug!(
        theme_mode = ?config.theme.mode,
        wallpaper_color = %config.wallpaper.color,
        wallpaper_path = config.wallpaper.path.as_ref().map(|path| path.display().to_string()).as_deref().unwrap_or("<none>"),
        backdrop_enabled = config.backdrop.enabled,
        "resolved startup configuration"
    );
    let gtk_app = gtk::Application::builder()
        .application_id(GTK_APPLICATION_ID)
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let _app_hold = gtk_app.hold();
    tracing::debug!(app_id = GTK_APPLICATION_ID, "starting GTK application");
    let app = RelmApp::from_app(gtk_app);
    app.visible_on_activate(false)
        .run::<WallpaperAppModel>(AppInit { config });
    tracing::info!("glimpse-wallpaper stopped");

    Ok(())
}

fn log_filter() -> EnvFilter {
    match std::env::var("GLIMPSE_LOG_LEVEL") {
        Ok(value) => normalized_glimpse_log_filter(&value)
            .unwrap_or_else(|| EnvFilter::new("info,relm4=warn")),
        Err(_) => {
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,relm4=warn"))
        }
    }
}

fn normalized_glimpse_log_filter(value: &str) -> Option<EnvFilter> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let filter = if value.contains(',') || value.contains('=') {
        value.to_string()
    } else {
        format!("{value},relm4=warn")
    };

    EnvFilter::try_new(filter).ok()
}

#[cfg(test)]
mod tests {
    use super::normalized_glimpse_log_filter;

    #[test]
    fn bare_glimpse_log_level_keeps_relm4_quiet() {
        let filter = normalized_glimpse_log_filter("debug").unwrap();
        let filter = filter.to_string();

        assert!(filter.contains("debug"));
        assert!(filter.contains("relm4=warn"));
    }

    #[test]
    fn explicit_glimpse_log_filter_is_preserved() {
        let filter = normalized_glimpse_log_filter("info,relm4=debug").unwrap();
        let filter = filter.to_string();

        assert!(filter.contains("info"));
        assert!(filter.contains("relm4=debug"));
    }
}
