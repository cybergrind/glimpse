use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use glimpse_core::{
    AppletConfig, AppletType, Config, ConfigDiscovery, KeyboardRememberMode, ThemeMode,
};

#[test]
fn config_discovery_prefers_glimpse_config_env_then_cwd_then_xdg() {
    let temp = TestDir::new("discovery-precedence");
    let env_file = temp.file("env/config.toml");
    let cwd_file = temp.file("cwd/config.toml");
    let xdg_file = temp.file("xdg/glimpse/config.toml");
    touch(&env_file);
    touch(&cwd_file);
    touch(&xdg_file);

    let discovery = ConfigDiscovery::new(
        HashMap::from([("GLIMPSE_CONFIG".into(), env_file.display().to_string())]),
        temp.path("cwd"),
        Some(temp.path("xdg")),
        Some(temp.path("home")),
    );

    assert_eq!(discovery.detect_config_file(), env_file);

    let discovery = ConfigDiscovery::new(
        HashMap::new(),
        temp.path("cwd"),
        Some(temp.path("xdg")),
        None,
    );
    assert_eq!(discovery.detect_config_file(), cwd_file);

    fs::remove_file(cwd_file).unwrap();
    assert_eq!(discovery.detect_config_file(), xdg_file);
}

#[test]
fn parses_shell_compatible_config_with_shared_wallpaper_settings() {
    let config = Config::from_toml_str(
        r##"
        theme = "adwaita"
        theme_mode = "dark"

        [location]
        provider = "static"
        latitude = 52.23
        longitude = 21.01

        [wallpaper]
        color = "#203040"
        path = "/tmp/wall.png"
        fit = "contain"
        transition_ms = 250
        mode = "image"

        [backdrop]
        enabled = true
        path = "/tmp/backdrop.png"
        blur_radius = 18

        [[panels]]
        position = "top"
        left = ["clock", "tray"]

        [applets.clock]
        format = "%H:%M"

        [applets.system_tray]
        extends = "tray"

        [applets.sysinfo]
        extends = "exec"
        command = ["/tmp/sysinfo"]
        "##,
    )
    .unwrap();

    assert_eq!(config.theme, "adwaita");
    assert_eq!(config.theme_mode, ThemeMode::Dark);
    assert_eq!(config.panels.len(), 1);
    assert!(matches!(
        config.applets.get("clock"),
        Some(AppletConfig { .. })
    ));
    assert_eq!(
        config
            .applets
            .get("system_tray")
            .and_then(|applet| applet.extends),
        Some(AppletType::Tray)
    );
    assert_eq!(
        config
            .applets
            .get("sysinfo")
            .and_then(|applet| applet.extends),
        Some(AppletType::Exec)
    );

    let serialized = toml::to_string_pretty(&config).unwrap();
    assert!(serialized.contains("[wallpaper]"));
    assert!(serialized.contains("[backdrop]"));
}

#[test]
fn parses_keyboard_service_config() {
    let config = Config::from_toml_str(
        r#"
        [keyboard]
        remember = "app"

        [keyboard.labels]
        us = "EN"
        "English (US)" = "🇺🇸"
        "#,
    )
    .unwrap();

    assert_eq!(config.keyboard.remember, KeyboardRememberMode::App);
    assert_eq!(config.keyboard.labels.get("us"), Some(&"EN".into()));
    assert_eq!(
        config.keyboard.labels.get("English (US)"),
        Some(&"🇺🇸".into())
    );
}

#[test]
fn config_ignores_unknown_applet_extends_values() {
    let config = Config::from_toml_str(
        r#"
        [[panels]]
        left = ["broken", "clock"]

        [applets.broken]
        extends = "made_up"
        command = ["/tmp/ignored"]
        "#,
    )
    .unwrap();

    assert_eq!(
        config
            .applets
            .get("broken")
            .and_then(|applet| applet.extends),
        None
    );
    assert_eq!(
        config.applets["broken"].settings["command"][0].as_str(),
        Some("/tmp/ignored")
    );
}

fn touch(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "").unwrap();
}

struct TestDir {
    root: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("glimpse-config-{name}-{suffix}"));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn file(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
