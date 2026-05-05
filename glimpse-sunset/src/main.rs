use glimpse_core::Config;
use glimpse_sunset::{app, runtime};
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    if let Some(output) = version_output(std::env::args()) {
        println!("{output}");
        return Ok(());
    }

    let filter = log_filter();
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let config = Config::load();
    tracing::debug!(
        schedule = ?config.night_light.schedule,
        temperature_kelvin = config.night_light.temperature,
        transition_minutes = config.night_light.transition_minutes,
        "resolved startup configuration"
    );
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            let _single_instance = match runtime::acquire_single_instance().await {
                Ok(guard) => {
                    tracing::info!("acquired single-instance D-Bus name");
                    guard
                }
                Err(error) => {
                    tracing::error!("failed to start glimpse-sunset: {error}");
                    return Err(error);
                }
            };

            app::run(config).await
        })
}

fn log_filter() -> EnvFilter {
    match std::env::var("GLIMPSE_LOG_LEVEL") {
        Ok(value) => {
            normalized_glimpse_log_filter(&value).unwrap_or_else(|| EnvFilter::new("info"))
        }
        Err(_) => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
    }
}

fn normalized_glimpse_log_filter(value: &str) -> Option<EnvFilter> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    EnvFilter::try_new(value).ok()
}

fn version_output<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .skip(1)
        .any(|arg| matches!(arg.as_ref(), "--version" | "-V"))
        .then(|| format!("glimpse-sunset {}", env!("CARGO_PKG_VERSION")))
}

#[cfg(test)]
mod tests {
    use super::{normalized_glimpse_log_filter, version_output};

    #[test]
    fn version_output_uses_cargo_package_version_for_long_flag() {
        assert_eq!(
            version_output(["glimpse-sunset", "--version"]),
            Some(format!("glimpse-sunset {}", env!("CARGO_PKG_VERSION")))
        );
    }

    #[test]
    fn version_output_uses_cargo_package_version_for_short_flag() {
        assert_eq!(
            version_output(["glimpse-sunset", "-V"]),
            Some(format!("glimpse-sunset {}", env!("CARGO_PKG_VERSION")))
        );
    }

    #[test]
    fn version_output_is_absent_without_flag() {
        assert_eq!(version_output(["glimpse-sunset"]), None);
    }

    #[test]
    fn bare_glimpse_log_level_is_accepted() {
        let filter = normalized_glimpse_log_filter("debug").unwrap();

        assert!(filter.to_string().contains("debug"));
    }
}
