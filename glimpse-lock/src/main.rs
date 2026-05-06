use glimpse_lock::{
    app::{self, LockAppConfig},
    config::LockConfig,
    logind,
    runtime::LockRuntime,
};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

const EXPORTED_LOCK_CSS: &str = include_str!("../resources/export-lock.css");
const EXPORTED_LOCK_CONFIG: &str = include_str!("../resources/lock.toml");

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(output) = version_output(&args) {
        println!("{output}");
        return Ok(());
    }
    if export_css_requested(&args) {
        let path = export_css()?;
        println!("wrote {}", path.display());
        return Ok(());
    }
    if export_config_requested(&args) {
        let path = export_config()?;
        println!("wrote {}", path.display());
        return Ok(());
    }
    let preview = preview_requested(&args);
    let gtk_args = gtk_args(&args);
    register_resources();

    tracing_subscriber::fmt()
        .with_env_filter(log_filter())
        .init();
    let config = LockAppConfig::load();
    let runtime = tokio::runtime::Runtime::new()?;
    let instance_guard = if preview {
        None
    } else {
        Some(runtime.block_on(LockRuntime::acquire_single_instance())?)
    };
    let result = if preview {
        tracing::info!("starting glimpse-lock preview; password 'valid' succeeds");
        let _runtime_guard = runtime.enter();
        app::run_preview(config, gtk_args)
    } else {
        let _runtime_guard = runtime.enter();
        app::run(
            config,
            gtk_args,
            instance_guard.as_ref().map(|guard| guard.connection()),
        )
    };
    if !preview {
        if let Err(error) = runtime.block_on(logind::set_current_session_locked_hint(false)) {
            tracing::debug!(%error, "failed to set logind LockedHint=false");
        }
    }
    result
}

fn register_resources() {
    gio::resources_register_include!("glimpse-lock.gresource")
        .expect("failed to register embedded lock resources");
}

fn preview_requested<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .skip(1)
        .any(|arg| matches!(arg.as_ref(), "--preview"))
}

fn export_css_requested<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .skip(1)
        .any(|arg| matches!(arg.as_ref(), "--export-css"))
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

fn gtk_args(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|arg| {
            !matches!(
                arg.as_str(),
                "--preview" | "--export-css" | "--export-config"
            )
        })
        .cloned()
        .collect()
}

fn export_css() -> anyhow::Result<PathBuf> {
    let path = LockConfig::config_dir().join("themes").join("lock.css");
    if path.exists() {
        anyhow::bail!(
            "lock CSS already exists at {}; refusing to overwrite",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, EXPORTED_LOCK_CSS)?;
    Ok(path)
}

fn export_config() -> anyhow::Result<PathBuf> {
    let path = LockConfig::config_file();
    if path.exists() {
        anyhow::bail!(
            "lock config already exists at {}; refusing to overwrite",
            path.display()
        );
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, EXPORTED_LOCK_CONFIG)?;
    Ok(path)
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
        .then(|| format!("glimpse-lock {}", env!("CARGO_PKG_VERSION")))
}

#[cfg(test)]
mod tests {
    use super::{
        export_config_requested, export_css_requested, gtk_args, preview_requested, version_output,
    };

    #[test]
    fn version_output_uses_cargo_package_version() {
        assert_eq!(
            version_output(["glimpse-lock", "--version"]),
            Some(format!("glimpse-lock {}", env!("CARGO_PKG_VERSION")))
        );
    }

    #[test]
    fn preview_flag_is_detected() {
        assert!(preview_requested(["glimpse-lock", "--preview"]));
        assert!(!preview_requested(["glimpse-lock"]));
    }

    #[test]
    fn export_css_flag_is_detected() {
        assert!(export_css_requested(["glimpse-lock", "--export-css"]));
        assert!(!export_css_requested(["glimpse-lock"]));
    }

    #[test]
    fn export_config_flag_is_detected() {
        assert!(export_config_requested(["glimpse-lock", "--export-config"]));
        assert!(!export_config_requested(["glimpse-lock"]));
    }

    #[test]
    fn glimpse_flags_are_removed_from_gtk_args() {
        let args = vec![
            "glimpse-lock".to_string(),
            "--preview".to_string(),
            "--export-css".to_string(),
            "--export-config".to_string(),
            "--gapplication-service".to_string(),
        ];

        assert_eq!(
            gtk_args(&args),
            vec![
                "glimpse-lock".to_string(),
                "--gapplication-service".to_string()
            ]
        );
    }
}
