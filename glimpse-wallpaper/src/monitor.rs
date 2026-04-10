//! Per-monitor layer-shell window component and config file watching.

use std::{collections::HashMap, path::PathBuf, rc::Rc};

use gtk4 as gtk;
use gtk::prelude::*;
use gtk::gdk;
use relm4::prelude::*;
use tracing::{info, warn};

use crate::config::{WallpaperConfig, WallpaperMode, resolve_config};
use crate::widgets::{build_wallpaper_widget, make_workspace_widget};

// ── MonitorWindow component ───────────────────────────────────────────────────

/// Initialization data for a per-monitor wallpaper window.
pub struct MonitorWindowInit {
    pub monitor: gdk::Monitor,
    /// Full root config (per-monitor overrides are resolved inside `init`).
    pub config: WallpaperConfig,
}

/// Per-monitor layer-shell window.
///
/// Holds the active workspace switcher when running in `workspace` mode;
/// `None` for all other modes (static image, video, etc.).
pub struct MonitorWindow {
    switcher: Option<Rc<dyn Fn(u8)>>,
}

#[derive(Debug)]
pub enum MonitorWindowMsg {
    SwitchWorkspace(u8),
}

impl SimpleComponent for MonitorWindow {
    type Init = MonitorWindowInit;
    type Input = MonitorWindowMsg;
    type Output = ();
    type Root = gtk::Window;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Window::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let connector = connector_name(&init.monitor);
        info!("wallpaper: opening window for monitor '{connector}'");

        setup_layer_shell(&root, &init.monitor);

        let config = init.config;
        let mon_cfg = config.monitors.iter().find(|m| m.name == connector);
        let is_workspace = mon_cfg
            .map(|m| matches!(m.mode, Some(WallpaperMode::Workspace)))
            .unwrap_or(false);

        let switcher = if is_workspace {
            let slots = mon_cfg.map(|m| m.workspaces.as_slice()).unwrap_or_default();
            let (widget, sw) = make_workspace_widget(slots, &config.color);
            root.set_child(Some(&widget));
            Some(sw)
        } else {
            let resolved = resolve_config(&config, &connector);
            root.set_child(Some(&build_wallpaper_widget(&resolved)));
            None
        };

        root.present();

        ComponentParts {
            model: MonitorWindow { switcher },
            widgets: (),
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            MonitorWindowMsg::SwitchWorkspace(idx) => {
                if let Some(switcher) = &self.switcher {
                    switcher(idx);
                }
            }
        }
    }
}

// ── Layer-shell setup ─────────────────────────────────────────────────────────

fn setup_layer_shell(window: &gtk::Window, monitor: &gdk::Monitor) {
    use gtk4_layer_shell::{Edge, Layer, LayerShell};

    window.init_layer_shell();
    window.set_layer(Layer::Background);
    window.set_namespace("glimpse-wallpaper");
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    window.set_exclusive_zone(-1);
    window.set_monitor(monitor);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_decorated(false);
    window.set_deletable(false);
}

// ── Batch helpers ─────────────────────────────────────────────────────────────

/// Opens a `MonitorWindow` for every monitor on `display` and returns the map.
pub fn open_all_monitors(
    display: &gdk::Display,
    config: &WallpaperConfig,
) -> HashMap<String, Controller<MonitorWindow>> {
    let monitors = display.monitors();
    let mut map = HashMap::new();

    for i in 0..monitors.n_items() {
        if let Some(mon) = monitors.item(i).and_downcast::<gdk::Monitor>() {
            let connector = connector_name(&mon);
            let ctrl = MonitorWindow::builder()
                .launch(MonitorWindowInit {
                    monitor: mon,
                    config: config.clone(),
                })
                .detach();
            map.insert(connector, ctrl);
        }
    }

    map
}

/// Returns the connector name of a monitor (e.g. `"DP-1"`).
pub fn connector_name(monitor: &gdk::Monitor) -> String {
    monitor
        .connector()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("unknown-{}", monitor.model().unwrap_or_default()))
}

// ── Config file watcher ───────────────────────────────────────────────────────

/// Watches the config file for changes and calls `on_change` on any modification.
///
/// Watches the parent directory (not the file itself) so that atomic editor
/// writes (temp-file + rename) are detected correctly on inotify-based systems.
///
/// `on_change` is called from the notify watcher thread (`Send + 'static`).
/// Typically sends `AppMsg::ReloadConfig` via the component sender.
pub fn start_config_watcher(
    config_path: PathBuf,
    on_change: impl Fn() + Send + 'static,
) {
    use notify::{EventKind, RecursiveMode, Watcher, recommended_watcher};

    let watch_dir = match config_path.parent() {
        Some(dir) => dir.to_path_buf(),
        None => {
            warn!("wallpaper: config path has no parent directory");
            return;
        }
    };

    let mut watcher = match recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            // Filter by filename only — avoids path canonicalization mismatches
            // (e.g. dirs::config_dir vs symlink-resolved inotify paths).
            let is_config = event
                .paths
                .iter()
                .any(|p| p.file_name().map(|n| n == "config.toml").unwrap_or(false));
            if is_config && matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                on_change();
            }
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            warn!("wallpaper: config watcher unavailable: {e}");
            return;
        }
    };

    if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
        warn!("wallpaper: cannot watch {}: {e}", watch_dir.display());
        return;
    }

    info!("wallpaper: watching {} for changes", watch_dir.display());
    std::mem::forget(watcher);
}
