use glimpse_wallpaper::{
    app::{AppInit, WallpaperAppModel},
    config::Config,
    runtime::{GTK_APPLICATION_ID, WallpaperRuntime},
};
use relm4::{
    RELM_THREADS, RelmApp,
    gtk::{self, gio::prelude::ApplicationExtManual},
};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

const GTK_APPLICATION_ID_ENV: &str = "GLIMPSE_WALLPAPER_APP_ID";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Some(output) = version_output(std::env::args()) {
        println!("{output}");
        return Ok(());
    }
    if export_config_requested(std::env::args()) {
        let path = export_config()?;
        println!("wrote {}", path.display());
        return Ok(());
    }

    let filter = log_filter();
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let threads = std::env::var("GLIMPSE_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4);
    RELM_THREADS.set(threads).ok();
    tracing::debug!(threads, "configured Relm4 worker threads");

    let app_id = gtk_application_id();
    let _single_instance = match WallpaperRuntime::acquire_single_instance_with_name(&app_id).await
    {
        Ok(guard) => {
            tracing::info!(app_id, "acquired single-instance D-Bus name");
            guard
        }
        Err(err) => {
            tracing::error!("failed to start glimpse-wallpaper: {err}");
            return Err(err);
        }
    };

    let config = Config::load();
    tracing::debug!(
        wallpaper_color = %config.wallpaper.color,
        wallpaper_path = config.wallpaper.path.as_ref().map(|path| path.display().to_string()).as_deref().unwrap_or("<none>"),
        backdrop_enabled = config.backdrop.enabled,
        "resolved startup configuration"
    );
    let gtk_app = gtk::Application::builder()
        .application_id(&app_id)
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let _app_hold = gtk_app.hold();
    tracing::debug!(app_id, "starting GTK application");
    let app = RelmApp::from_app(gtk_app);
    app.visible_on_activate(false)
        .run::<WallpaperAppModel>(AppInit { config });
    tracing::info!("glimpse-wallpaper stopped");

    Ok(())
}

fn gtk_application_id() -> String {
    gtk_application_id_from_env(std::env::var(GTK_APPLICATION_ID_ENV).ok())
}

fn gtk_application_id_from_env(value: Option<String>) -> String {
    value.unwrap_or_else(|| GTK_APPLICATION_ID.into())
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

fn version_output<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .skip(1)
        .any(|arg| matches!(arg.as_ref(), "--version" | "-V"))
        .then(|| format!("glimpse-wallpaper {}", env!("CARGO_PKG_VERSION")))
}

fn export_config_requested<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .skip(1)
        .any(|arg| matches!(arg.as_ref(), "--export-config"))
}

fn export_config() -> anyhow::Result<PathBuf> {
    let path = Config::config_file();
    if path.exists() {
        anyhow::bail!(
            "wallpaper config already exists at {}; refusing to overwrite",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, glimpse_wallpaper::config::EXPORTED_CONFIG)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::{
        export_config_requested, gtk_application_id_from_env, normalized_glimpse_log_filter,
        version_output,
    };

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

    #[test]
    fn wallpaper_app_id_defaults_to_runtime_constant() {
        assert_eq!(gtk_application_id_from_env(None), super::GTK_APPLICATION_ID);
    }

    #[test]
    fn wallpaper_app_id_can_be_overridden_from_env() {
        assert_eq!(
            gtk_application_id_from_env(Some("me.aresa.GlimpseWallpaper.TestApp".into())),
            "me.aresa.GlimpseWallpaper.TestApp"
        );
    }

    #[test]
    fn version_output_uses_cargo_package_version_for_long_flag() {
        assert_eq!(
            version_output(["glimpse-wallpaper", "--version"]),
            Some(format!("glimpse-wallpaper {}", env!("CARGO_PKG_VERSION")))
        );
    }

    #[test]
    fn version_output_uses_cargo_package_version_for_short_flag() {
        assert_eq!(
            version_output(["glimpse-wallpaper", "-V"]),
            Some(format!("glimpse-wallpaper {}", env!("CARGO_PKG_VERSION")))
        );
    }

    #[test]
    fn version_output_is_absent_without_flag() {
        assert_eq!(version_output(["glimpse-wallpaper"]), None);
    }

    #[test]
    fn export_config_flag_is_detected() {
        assert!(export_config_requested([
            "glimpse-wallpaper",
            "--export-config"
        ]));
        assert!(!export_config_requested(["glimpse-wallpaper"]));
    }
}
