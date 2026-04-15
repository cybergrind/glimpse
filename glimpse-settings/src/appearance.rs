use gio::prelude::*;
use glimpse::config::{BackdropConfig, Config, WallpaperConfig, WallpaperMode};
use gtk4::{gio, glib};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Default,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentColor {
    Blue,
    Teal,
    Green,
    Yellow,
    Orange,
    Red,
    Pink,
    Purple,
    Slate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeKind {
    Gtk,
    Icon,
    Cursor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppearanceDraft {
    pub color_scheme: ColorScheme,
    pub accent_color: AccentColor,
    pub gtk_theme: String,
    pub icon_theme: String,
    pub cursor_theme: String,
    pub interface_font: String,
    pub monospace_font: String,
    pub text_scale: f64,
    pub wallpaper: WallpaperConfig,
    pub backdrop: BackdropConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeOption {
    pub name: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalAppearanceUpdate {
    Unchanged,
    SyncedClean,
    BaselineUpdated,
}

pub const INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";
pub const COLOR_SCHEME_KEY: &str = "color-scheme";
pub const ACCENT_COLOR_KEY: &str = "accent-color";
pub const GTK_THEME_KEY: &str = "gtk-theme";
pub const ICON_THEME_KEY: &str = "icon-theme";
pub const CURSOR_THEME_KEY: &str = "cursor-theme";
pub const FONT_KEY: &str = "font-name";
pub const MONOSPACE_FONT_KEY: &str = "monospace-font-name";
pub const TEXT_SCALING_KEY: &str = "text-scaling-factor";

#[derive(Clone)]
pub struct AppearanceSettings {
    settings: gio::Settings,
}

impl ColorScheme {
    pub fn all() -> &'static [ColorScheme] {
        &[Self::Default, Self::Light, Self::Dark]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "System Default",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn gsettings_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Light => "prefer-light",
            Self::Dark => "prefer-dark",
        }
    }

    pub fn from_gsettings_value(value: &str) -> Self {
        match value {
            "prefer-light" => Self::Light,
            "prefer-dark" => Self::Dark,
            _ => Self::Default,
        }
    }
}

impl AccentColor {
    pub fn all() -> &'static [AccentColor] {
        &[
            Self::Blue,
            Self::Teal,
            Self::Green,
            Self::Yellow,
            Self::Orange,
            Self::Red,
            Self::Pink,
            Self::Purple,
            Self::Slate,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Blue => "Blue",
            Self::Teal => "Teal",
            Self::Green => "Green",
            Self::Yellow => "Yellow",
            Self::Orange => "Orange",
            Self::Red => "Red",
            Self::Pink => "Pink",
            Self::Purple => "Purple",
            Self::Slate => "Slate",
        }
    }

    pub fn gsettings_value(self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Teal => "teal",
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Orange => "orange",
            Self::Red => "red",
            Self::Pink => "pink",
            Self::Purple => "purple",
            Self::Slate => "slate",
        }
    }

    pub fn from_gsettings_value(value: &str) -> Self {
        match value {
            "teal" => Self::Teal,
            "green" => Self::Green,
            "yellow" => Self::Yellow,
            "orange" => Self::Orange,
            "red" => Self::Red,
            "pink" => Self::Pink,
            "purple" => Self::Purple,
            "slate" => Self::Slate,
            _ => Self::Blue,
        }
    }
}

pub fn discover_theme_options(
    kind: ThemeKind,
    roots: &[PathBuf],
    current: Option<&str>,
) -> Vec<ThemeOption> {
    let mut names = Vec::<String>::new();

    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !theme_name_allowed(kind, &path) {
                continue;
            }

            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if !names.iter().any(|existing| existing == name) {
                names.push(name.to_string());
            }
        }
    }

    names.sort_unstable_by_key(|value| value.to_lowercase());

    let mut options = names
        .into_iter()
        .map(|name| ThemeOption {
            name,
            installed: true,
        })
        .collect::<Vec<_>>();

    if let Some(current) = current.filter(|value| !value.trim().is_empty()) {
        if !options.iter().any(|option| option.name == current) {
            options.insert(
                0,
                ThemeOption {
                    name: current.to_string(),
                    installed: false,
                },
            );
        }
    }

    options
}

impl AppearanceDraft {
    pub fn is_dirty_against(&self, baseline: &Self) -> bool {
        self != baseline
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.text_scale < 0.5 || self.text_scale > 3.0 {
            return Err("text scale out of range");
        }

        if self.gtk_theme.is_empty()
            || self.icon_theme.is_empty()
            || self.cursor_theme.is_empty()
            || self.interface_font.is_empty()
            || self.monospace_font.is_empty()
        {
            return Err("required fields must not be empty");
        }

        if self.wallpaper.mode == WallpaperMode::Image && self.wallpaper.path.is_none() {
            return Err("choose a wallpaper image or switch to solid color");
        }

        if self.backdrop.enabled
            && self.backdrop.path.is_none()
            && self.wallpaper.mode != WallpaperMode::Image
        {
            return Err("choose a backdrop image or use an image wallpaper");
        }

        Ok(())
    }
}

impl AppearanceSettings {
    pub fn new() -> Self {
        Self {
            settings: gio::Settings::new(INTERFACE_SCHEMA),
        }
    }

    pub fn snapshot(&self) -> AppearanceDraft {
        let config = Config::load();
        AppearanceDraft {
            color_scheme: ColorScheme::from_gsettings_value(
                self.settings.string(COLOR_SCHEME_KEY).as_str(),
            ),
            accent_color: AccentColor::from_gsettings_value(
                self.settings.string(ACCENT_COLOR_KEY).as_str(),
            ),
            gtk_theme: self.settings.string(GTK_THEME_KEY).to_string(),
            icon_theme: self.settings.string(ICON_THEME_KEY).to_string(),
            cursor_theme: self.settings.string(CURSOR_THEME_KEY).to_string(),
            interface_font: self.settings.string(FONT_KEY).to_string(),
            monospace_font: self.settings.string(MONOSPACE_FONT_KEY).to_string(),
            text_scale: self.settings.double(TEXT_SCALING_KEY),
            wallpaper: config.wallpaper,
            backdrop: config.backdrop,
        }
    }

    pub fn apply_desktop_settings(&self, draft: &AppearanceDraft) -> Result<(), glib::BoolError> {
        self.settings
            .set_string(COLOR_SCHEME_KEY, draft.color_scheme.gsettings_value())?;
        self.settings
            .set_string(ACCENT_COLOR_KEY, draft.accent_color.gsettings_value())?;
        self.settings.set_string(GTK_THEME_KEY, &draft.gtk_theme)?;
        self.settings
            .set_string(ICON_THEME_KEY, &draft.icon_theme)?;
        self.settings
            .set_string(CURSOR_THEME_KEY, &draft.cursor_theme)?;
        self.settings.set_string(FONT_KEY, &draft.interface_font)?;
        self.settings
            .set_string(MONOSPACE_FONT_KEY, &draft.monospace_font)?;
        self.settings
            .set_double(TEXT_SCALING_KEY, draft.text_scale)?;
        Ok(())
    }

    pub fn connect_changed<F: Fn() + Clone + 'static>(&self, f: F) -> Vec<glib::SignalHandlerId> {
        let mut ids = Vec::new();
        for key in [
            COLOR_SCHEME_KEY,
            ACCENT_COLOR_KEY,
            GTK_THEME_KEY,
            ICON_THEME_KEY,
            CURSOR_THEME_KEY,
            FONT_KEY,
            MONOSPACE_FONT_KEY,
            TEXT_SCALING_KEY,
        ] {
            let callback = f.clone();
            ids.push(self.settings.connect_changed(Some(key), move |_, _| {
                callback();
            }));
        }
        ids
    }
}

pub fn reconcile_external_snapshot(
    draft: &mut AppearanceDraft,
    baseline: &mut AppearanceDraft,
    snapshot: AppearanceDraft,
) -> ExternalAppearanceUpdate {
    let is_dirty = draft != baseline;
    if *baseline == snapshot && (!is_dirty || *draft == snapshot) {
        return ExternalAppearanceUpdate::Unchanged;
    }

    if is_dirty {
        *baseline = snapshot;
        ExternalAppearanceUpdate::BaselineUpdated
    } else {
        *baseline = snapshot.clone();
        *draft = snapshot;
        ExternalAppearanceUpdate::SyncedClean
    }
}

pub fn theme_search_roots(_kind: ThemeKind) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let Some(home) = std::env::var_os("HOME") else {
        return roots;
    };
    let home = PathBuf::from(home);

    match _kind {
        ThemeKind::Gtk => {
            roots.push(home.join(".themes"));
            roots.push(home.join(".local/share/themes"));
            roots.push(PathBuf::from("/usr/share/themes"));
        }
        ThemeKind::Icon | ThemeKind::Cursor => {
            roots.push(home.join(".icons"));
            roots.push(home.join(".local/share/icons"));
            roots.push(PathBuf::from("/usr/share/icons"));
        }
    }

    roots
}

pub fn theme_name_allowed(kind: ThemeKind, path: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }

    match kind {
        ThemeKind::Gtk => true,
        ThemeKind::Icon => path.join("index.theme").exists() || path.join("cursors").is_dir(),
        ThemeKind::Cursor => path.join("cursors").is_dir(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AccentColor, AppearanceDraft, BackdropConfig, ColorScheme, ExternalAppearanceUpdate,
        ThemeKind, WallpaperConfig, discover_theme_options, reconcile_external_snapshot,
        theme_name_allowed, theme_search_roots,
    };
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should work")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("glimpse-settings-{label}-{unique}"));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    #[test]
    fn color_scheme_and_accent_color_use_gsettings_values() {
        assert_eq!(ColorScheme::Default.gsettings_value(), "default");
        assert_eq!(ColorScheme::Light.gsettings_value(), "prefer-light");
        assert_eq!(ColorScheme::Dark.gsettings_value(), "prefer-dark");
        assert_eq!(AccentColor::Slate.gsettings_value(), "slate");
    }

    #[test]
    fn theme_discovery_dedupes_and_preserves_missing_current_value() {
        let root = temp_dir("theme-discovery");
        fs::create_dir(root.join("Adwaita")).expect("gtk theme should be created");
        fs::create_dir(root.join("Adwaita-dark")).expect("gtk theme should be created");
        fs::create_dir(root.join("Adwaita-copy")).expect("theme should be created");
        fs::create_dir(root.join("Adwaita-copy-2")).expect("theme should be created");
        fs::create_dir(root.join("Adwaita-copy").join("gtk-4.0"))
            .expect("gtk dir should be created");
        fs::create_dir(root.join("Adwaita-copy-2").join("gtk-4.0"))
            .expect("gtk dir should be created");

        let options =
            discover_theme_options(ThemeKind::Gtk, &[root.clone()], Some("Missing Theme"));

        assert!(options.iter().any(|item| item.name == "Adwaita"));
        assert!(options.iter().any(|item| item.name == "Adwaita-dark"));
        assert!(
            options
                .iter()
                .any(|item| item.name == "Missing Theme" && !item.installed)
        );
        assert_eq!(
            options
                .iter()
                .filter(|item| item.name == "Adwaita-copy")
                .count(),
            1
        );
    }

    #[test]
    fn cursor_discovery_requires_cursor_data() {
        let root = temp_dir("cursor-discovery");
        fs::create_dir(root.join("PlainIcons")).expect("icon dir should be created");
        fs::create_dir(root.join("CursorTheme")).expect("cursor dir should be created");
        fs::create_dir(root.join("CursorTheme").join("cursors"))
            .expect("cursors dir should be created");

        assert!(!theme_name_allowed(
            ThemeKind::Cursor,
            &root.join("PlainIcons")
        ));
        assert!(theme_name_allowed(
            ThemeKind::Cursor,
            &root.join("CursorTheme")
        ));
    }

    #[test]
    fn text_scale_validation_matches_schema_range() {
        let draft = AppearanceDraft {
            color_scheme: ColorScheme::Dark,
            accent_color: AccentColor::Blue,
            gtk_theme: "Adwaita".into(),
            icon_theme: "Adwaita".into(),
            cursor_theme: "Adwaita".into(),
            interface_font: "Noto Sans 10".into(),
            monospace_font: "Hack 10".into(),
            text_scale: 3.5,
            wallpaper: WallpaperConfig::default(),
            backdrop: BackdropConfig::default(),
        };

        assert_eq!(draft.validate(), Err("text scale out of range"));
    }

    #[test]
    fn search_roots_cover_user_and_system_theme_locations() {
        let roots = theme_search_roots(ThemeKind::Gtk);

        assert!(roots.iter().any(|path| path.ends_with(".themes")));
        assert!(
            roots
                .iter()
                .any(|path| path.ends_with(".local/share/themes"))
        );
        assert!(
            roots
                .iter()
                .any(|path| path == &PathBuf::from("/usr/share/themes"))
        );
    }

    #[test]
    fn reconcile_external_snapshot_replaces_clean_draft() {
        let mut baseline = sample_draft();
        let mut draft = baseline.clone();
        let mut external = baseline.clone();
        external.color_scheme = ColorScheme::Light;

        let update = reconcile_external_snapshot(&mut draft, &mut baseline, external.clone());

        assert_eq!(update, ExternalAppearanceUpdate::SyncedClean);
        assert_eq!(draft, external);
        assert_eq!(baseline, external);
    }

    #[test]
    fn reconcile_external_snapshot_only_updates_baseline_when_dirty() {
        let mut baseline = sample_draft();
        let mut draft = baseline.clone();
        draft.text_scale = 1.25;
        let mut external = baseline.clone();
        external.color_scheme = ColorScheme::Light;

        let update = reconcile_external_snapshot(&mut draft, &mut baseline, external.clone());

        assert_eq!(update, ExternalAppearanceUpdate::BaselineUpdated);
        assert_eq!(draft.text_scale, 1.25);
        assert_eq!(baseline, external);
    }

    fn sample_draft() -> AppearanceDraft {
        AppearanceDraft {
            color_scheme: ColorScheme::Dark,
            accent_color: AccentColor::Blue,
            gtk_theme: "Adwaita".into(),
            icon_theme: "Adwaita".into(),
            cursor_theme: "Adwaita".into(),
            interface_font: "Noto Sans 10".into(),
            monospace_font: "Hack 10".into(),
            text_scale: 1.0,
            wallpaper: WallpaperConfig::default(),
            backdrop: BackdropConfig::default(),
        }
    }
}
