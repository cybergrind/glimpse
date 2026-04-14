use std::{
    fs,
    path::{Path, PathBuf},
};

use relm4::gtk::{self, CssProvider, gdk::RGBA, prelude::*};

use glimpse::config::{Config, ThemeMode};

const EMBEDDED_BASE_CSS: &str = include_str!("../../../themes/base.css");
const EMBEDDED_STRUCTURE_CSS: &str = include_str!("../../../themes/structure.css");
const EMBEDDED_ACCENT_CSS: &str = include_str!("../../../themes/accent.css");
const EMBEDDED_ADWAITA_CSS: &str = include_str!("../../../themes/adwaita.css");

const DEFAULT_THEME_NAME: &str = "adwaita";

pub(super) fn sync_base_css(provider: &CssProvider) {
    provider.load_from_data("");

    #[cfg(feature = "dev")]
    if let Some(path) = dev_theme_path("base.css").filter(|path| path.exists() && path.is_file()) {
        provider.load_from_path(&path);
        tracing::info!("loaded base css from {}", path.display());
        return;
    }

    provider.load_from_data(EMBEDDED_BASE_CSS);
    tracing::info!("loaded embedded base css");
}

pub(super) fn sync_structure_css(provider: &CssProvider) {
    provider.load_from_data("");

    #[cfg(feature = "dev")]
    if let Some(path) =
        dev_theme_path("structure.css").filter(|path| path.exists() && path.is_file())
    {
        provider.load_from_path(&path);
        tracing::info!("loaded structure css from {}", path.display());
        return;
    }

    provider.load_from_data(EMBEDDED_STRUCTURE_CSS);
    tracing::info!("loaded embedded structure css");
}

pub(super) fn sync_accent_css(provider: &CssProvider) {
    provider.load_from_data("");
    provider.load_from_data(EMBEDDED_ACCENT_CSS);
    tracing::info!("loaded accent css");
}

pub(super) fn sync_theme_css(provider: &CssProvider, config: &Config) {
    provider.load_from_data("");

    let default_path = match ensure_default_theme_file() {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(error = %error, "failed to sync default adwaita theme");
            Config::theme_path_for_name(DEFAULT_THEME_NAME)
        }
    };

    let requested_path = config.active_theme_path();
    let resolved_path = resolve_theme_path(requested_path.clone(), default_path.clone());

    if resolved_path.exists() && resolved_path.is_file() {
        provider.load_from_path(&resolved_path);
        if requested_path.as_ref() == Some(&resolved_path) {
            tracing::info!("loaded theme css from {}", resolved_path.display());
        } else if let Some(requested_path) = requested_path {
            tracing::warn!(
                requested = %requested_path.display(),
                fallback = %resolved_path.display(),
                "requested theme was unavailable; loaded adwaita instead"
            );
        } else {
            tracing::info!("loaded default theme css from {}", resolved_path.display());
        }
    } else {
        tracing::warn!("theme css file not found: {}", resolved_path.display());
    }
}

pub(super) fn apply_theme_mode(widget: &impl IsA<gtk::Widget>, mode: ThemeMode) {
    let widget = widget.as_ref();
    widget.remove_css_class("theme-light");
    widget.remove_css_class("theme-dark");

    match mode {
        ThemeMode::System => {}
        ThemeMode::Light => widget.add_css_class("theme-light"),
        ThemeMode::Dark => widget.add_css_class("theme-dark"),
    }
}

pub(super) fn render_accent_css(accent: &RGBA) -> String {
    let accent_hex = rgba_to_hex(accent);
    let foreground = accent_foreground(accent);
    format!(
        ":root {{\n    --sys-accent: {accent_hex};\n    --sys-accent-fg: {foreground};\n}}\n"
    )
}

pub(super) fn resolve_theme_path(requested: Option<PathBuf>, default_path: PathBuf) -> PathBuf {
    match requested {
        Some(path) if path.exists() && path.is_file() => path,
        _ => default_path,
    }
}

pub(super) fn sync_theme_file(path: &Path, contents: &[u8]) -> std::io::Result<bool> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    if fs::read(path).ok().as_deref() == Some(contents) {
        return Ok(false);
    }

    fs::write(path, contents)?;
    Ok(true)
}

fn ensure_default_theme_file() -> std::io::Result<PathBuf> {
    let path = Config::theme_path_for_name(DEFAULT_THEME_NAME);
    let contents = default_theme_source_bytes();
    let updated = sync_theme_file(&path, &contents)?;

    if updated {
        tracing::info!("synced default theme to {}", path.display());
    }

    Ok(path)
}

fn default_theme_source_bytes() -> Vec<u8> {
    #[cfg(feature = "dev")]
    if let Some(path) = dev_theme_path("adwaita.css").filter(|path| path.exists() && path.is_file())
    {
        if let Ok(bytes) = fs::read(&path) {
            return bytes;
        }
    }

    EMBEDDED_ADWAITA_CSS.as_bytes().to_vec()
}

fn rgba_to_hex(rgba: &RGBA) -> String {
    let to_byte = |component: f32| (component.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        to_byte(rgba.red()),
        to_byte(rgba.green()),
        to_byte(rgba.blue()),
    )
}

fn accent_foreground(accent: &RGBA) -> &'static str {
    let linear = |component: f32| {
        if component <= 0.04045 {
            component / 12.92
        } else {
            ((component + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance =
        0.2126 * linear(accent.red()) + 0.7152 * linear(accent.green()) + 0.0722 * linear(accent.blue());

    if luminance > 0.6 {
        "#000000"
    } else {
        "#ffffff"
    }
}

#[cfg(feature = "dev")]
pub(super) fn repo_themes_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root should exist")
        .join("themes")
}

#[cfg(feature = "dev")]
pub(super) fn is_repo_theme_css_change(path: &Path) -> bool {
    path.strip_prefix(repo_themes_directory())
        .ok()
        .is_some_and(|_| path.extension().and_then(|ext| ext.to_str()) == Some("css"))
}

#[cfg(feature = "dev")]
fn dev_theme_path(name: &str) -> Option<PathBuf> {
    Some(repo_themes_directory().join(name))
}

#[cfg(not(feature = "dev"))]
fn dev_theme_path(_name: &str) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn sync_theme_file_writes_missing_and_changed_default_theme() {
        let dir = unique_temp_dir("glimpse-theme-runtime-sync");
        let path = dir.join("adwaita.css");

        assert!(sync_theme_file(&path, b"first").expect("initial sync should work"));
        assert_eq!(fs::read(&path).expect("theme file should exist"), b"first");

        assert!(!sync_theme_file(&path, b"first").expect("identical theme should not rewrite"));
        assert!(sync_theme_file(&path, b"second").expect("changed theme should rewrite"));
        assert_eq!(fs::read(&path).expect("theme file should update"), b"second");
    }

    #[test]
    fn resolve_theme_path_falls_back_to_default_when_requested_theme_is_missing() {
        let dir = unique_temp_dir("glimpse-theme-runtime-resolve");
        let default_path = dir.join("adwaita.css");
        fs::write(&default_path, "default").expect("default theme should write");

        let resolved = resolve_theme_path(Some(dir.join("missing.css")), default_path.clone());

        assert_eq!(resolved, default_path);
    }

    #[test]
    fn generated_accent_css_exposes_system_tokens_with_contrast_foreground() {
        let accent = RGBA::new(0.9, 0.9, 0.9, 1.0);
        let css = render_accent_css(&accent);

        assert!(css.contains("--sys-accent:"));
        assert!(css.contains("--sys-accent-fg:"));
        assert!(css.contains("#000000"));
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        dir
    }
}
