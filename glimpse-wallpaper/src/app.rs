use std::{
    collections::{HashMap, HashSet},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use css_color::Srgb;
use glimpse_core::{
    Config, ConfigEvent, FitMode, ResolvedBackdropSpec, ResolvedImageSpec, ResolvedWallpaperSpec,
    heic, watch_for_config_changes,
};
use gtk4::prelude::ListModelExt;
use gtk4::{
    ContentFit,
    gdk::{self, prelude::MonitorExt},
    glib::{
        self,
        object::{Cast, ObjectExt, ObjectType},
    },
    prelude::{DisplayExt, DrawingAreaExtManual, GtkWindowExt, WidgetExt},
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::runtime::{BACKDROP_NAMESPACE, WALLPAPER_NAMESPACE};

#[derive(Debug)]
pub enum AppCommand {
    ApplyResolvedSpec(ResolvedWallpaperSpec),
    ReloadConfig,
    ReloadAssets,
    MonitorsChanged,
}

#[derive(Default)]
pub struct WallpaperAppModel {
    active_spec: Option<ResolvedWallpaperSpec>,
    wallpaper_windows: HashMap<MonitorKey, Controller<WallpaperWindow>>,
    backdrop_windows: HashMap<MonitorKey, Controller<BackdropWindow>>,
    subscribed_monitors: HashSet<MonitorKey>,
    asset_watch_cancel: Option<CancellationToken>,
    ready_logged: bool,
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
struct MonitorKey(usize);

impl MonitorKey {
    fn for_monitor(monitor: &gdk::Monitor) -> Self {
        Self(monitor.as_ptr() as usize)
    }
}

impl WallpaperAppModel {
    pub fn update(&mut self, command: AppCommand) {
        if let AppCommand::ApplyResolvedSpec(spec) = command {
            self.active_spec = Some(spec);
        }
    }

    pub fn active_spec(&self) -> Option<&ResolvedWallpaperSpec> {
        self.active_spec.as_ref()
    }
}

pub struct AppInit {
    pub config: Config,
}

#[relm4::component(pub)]
impl SimpleComponent for WallpaperAppModel {
    type Init = AppInit;
    type Input = AppCommand;
    type Output = ();

    view! {
        gtk::Window {
            set_visible: false,
            set_decorated: false,
            set_deletable: false,
            set_resizable: false,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.init_layer_shell();
        root.set_layer(Layer::Background);
        root.set_namespace(Some(WALLPAPER_NAMESPACE));
        root.set_keyboard_mode(KeyboardMode::None);
        root.set_default_size(-1, -1);
        root.set_opacity(0.0);
        tracing::debug!("initialized invisible wallpaper app root window");

        let (config_tx, mut config_rx) = mpsc::channel(1);
        relm4::spawn(async move {
            tracing::debug!("starting configuration watcher");
            watch_for_config_changes(config_tx).await;
        });

        let config_sender = sender.clone();
        relm4::spawn(async move {
            while let Some(ConfigEvent::Changed(config)) = config_rx.recv().await {
                tracing::info!("configuration changed, applying wallpaper settings");
                let spec = config.resolve_wallpaper();
                let _ = config_sender.input(AppCommand::ApplyResolvedSpec(spec));
            }
        });

        if let Some(display) = gdk::Display::default() {
            let monitor_count = display.monitors().n_items();
            tracing::debug!(monitor_count, "connected to default GDK display");
            let monitor_sender = sender.clone();
            let _ = monitor_sender.input(AppCommand::MonitorsChanged);
            display.monitors().connect_items_changed(move |_, _, _, _| {
                tracing::info!("monitor list changed, reconciling wallpaper surfaces");
                let _ = monitor_sender.input(AppCommand::MonitorsChanged);
            });
        } else {
            tracing::warn!("no default GDK display; wallpaper surfaces cannot be created yet");
        }

        let initial_spec = init.config.resolve_wallpaper();
        tracing::info!(
            color = %initial_spec.color,
            image = initial_spec.image.as_ref().map(|image| image.path.display().to_string()).as_deref().unwrap_or("<none>"),
            transition_ms = initial_spec.transition_ms,
            backdrop = backdrop_label(&initial_spec.backdrop),
            "applying initial wallpaper spec"
        );
        let _ = sender.input(AppCommand::ApplyResolvedSpec(initial_spec));

        let model = WallpaperAppModel::default();
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            AppCommand::ApplyResolvedSpec(spec) => {
                tracing::debug!("received resolved wallpaper spec");
                self.apply_resolved_spec(spec, false, sender);
            }
            AppCommand::ReloadConfig => {
                tracing::info!("reloading wallpaper configuration");
                let config = Config::load();
                let spec = config.resolve_wallpaper();
                let _ = sender.input(AppCommand::ApplyResolvedSpec(spec));
            }
            AppCommand::ReloadAssets => {
                tracing::info!("reloading wallpaper assets");
                let config = Config::load();
                let spec = config.resolve_wallpaper();
                self.apply_resolved_spec(spec, true, sender);
            }
            AppCommand::MonitorsChanged => {
                if let Some(spec) = self.active_spec.clone() {
                    tracing::debug!("reconciling wallpaper surfaces after monitor change");
                    self.reconcile_windows(&spec, false, sender);
                }
            }
        }
    }
}

impl WallpaperAppModel {
    fn apply_resolved_spec(
        &mut self,
        spec: ResolvedWallpaperSpec,
        force_image_reload: bool,
        sender: ComponentSender<Self>,
    ) {
        if !force_image_reload && self.active_spec.as_ref() == Some(&spec) {
            tracing::debug!("wallpaper spec unchanged; skipping reconfigure");
            return;
        }
        tracing::info!(
            color = %spec.color,
            image = spec.image.as_ref().map(|image| image.path.display().to_string()).as_deref().unwrap_or("<none>"),
            transition_ms = spec.transition_ms,
            force_image_reload,
            backdrop = backdrop_label(&spec.backdrop),
            "applying wallpaper spec"
        );
        self.active_spec = Some(spec.clone());
        self.reconcile_windows(&spec, force_image_reload, sender.clone());
        self.watch_active_paths(spec, sender);
    }

    fn reconcile_windows(
        &mut self,
        spec: &ResolvedWallpaperSpec,
        force_image_reload: bool,
        sender: ComponentSender<Self>,
    ) {
        let Some(display) = gdk::Display::default() else {
            tracing::warn!("no default GDK display while reconciling wallpaper surfaces");
            return;
        };
        let all_monitors = list_monitors(&display);
        let (monitors, pending): (Vec<_>, Vec<_>) = all_monitors
            .into_iter()
            .partition(|monitor| monitor_target_size(monitor) != (0, 0));

        for monitor in &pending {
            let key = MonitorKey::for_monitor(monitor);
            if self.subscribed_monitors.insert(key) {
                let label = connector_name(monitor);
                tracing::info!(
                    monitor = %label,
                    "monitor announced with zero geometry; deferring surface until geometry arrives"
                );
                let notify_sender = sender.clone();
                monitor.connect_notify_local(Some("geometry"), move |_, _| {
                    let _ = notify_sender.input(AppCommand::MonitorsChanged);
                });
            }
        }

        let live_keys: HashSet<MonitorKey> = monitors
            .iter()
            .chain(pending.iter())
            .map(MonitorKey::for_monitor)
            .collect();
        self.subscribed_monitors.retain(|key| live_keys.contains(key));

        if monitors.is_empty() {
            tracing::warn!(
                pending_count = pending.len(),
                "no monitors with non-zero geometry; no wallpaper surfaces created"
            );
        } else {
            tracing::info!(
                monitor_count = monitors.len(),
                pending_count = pending.len(),
                force_image_reload,
                "reconciling wallpaper surfaces"
            );
        }
        self.reconcile_backdrop_windows(&monitors, spec, force_image_reload);

        let mut existing_wallpaper = std::mem::take(&mut self.wallpaper_windows);
        let mut next_wallpaper = HashMap::new();

        for monitor in &monitors {
            let key = MonitorKey::for_monitor(monitor);
            let label = connector_name(monitor);
            let controller = match existing_wallpaper.remove(&key) {
                Some(controller) => {
                    tracing::debug!(monitor = %label, "reconfiguring wallpaper surface");
                    controller.emit(WallpaperWindowInput::Reconfigure {
                        spec: spec.clone(),
                        target_size: monitor_target_size(monitor),
                        force_image_reload,
                    });
                    controller
                }
                None => {
                    tracing::info!(monitor = %label, "creating wallpaper surface");
                    WallpaperWindow::builder()
                        .launch(WallpaperWindowInit {
                            monitor: monitor.clone(),
                            spec: spec.clone(),
                        })
                        .detach()
                }
            };
            next_wallpaper.insert(key, controller);
        }
        self.wallpaper_windows = next_wallpaper;

        if !self.ready_logged {
            tracing::info!(
                wallpaper_surfaces = self.wallpaper_windows.len(),
                backdrop_surfaces = self.backdrop_windows.len(),
                "glimpse-wallpaper is running"
            );
            self.ready_logged = true;
        } else {
            tracing::debug!(
                wallpaper_surfaces = self.wallpaper_windows.len(),
                backdrop_surfaces = self.backdrop_windows.len(),
                "wallpaper surface reconciliation complete"
            );
        }
    }

    fn reconcile_backdrop_windows(
        &mut self,
        monitors: &[gdk::Monitor],
        spec: &ResolvedWallpaperSpec,
        force_image_reload: bool,
    ) {
        if matches!(spec.backdrop, ResolvedBackdropSpec::Disabled) {
            if !self.backdrop_windows.is_empty() {
                tracing::info!("backdrop disabled, removing backdrop surfaces");
            }
            self.backdrop_windows.clear();
            return;
        }

        let mut existing = std::mem::take(&mut self.backdrop_windows);
        let mut next = HashMap::new();
        for monitor in monitors {
            let key = MonitorKey::for_monitor(monitor);
            let label = connector_name(monitor);
            let controller = match existing.remove(&key) {
                Some(controller) => {
                    tracing::debug!(monitor = %label, "reconfiguring backdrop surface");
                    controller.emit(BackdropWindowInput::Reconfigure {
                        backdrop: spec.backdrop.clone(),
                        target_size: monitor_target_size(monitor),
                        force_image_reload,
                    });
                    controller
                }
                None => {
                    tracing::info!(monitor = %label, "creating backdrop surface");
                    BackdropWindow::builder()
                        .launch(BackdropWindowInit {
                            monitor: monitor.clone(),
                            backdrop: spec.backdrop.clone(),
                        })
                        .detach()
                }
            };
            next.insert(key, controller);
        }
        self.backdrop_windows = next;
    }

    fn watch_active_paths(&mut self, spec: ResolvedWallpaperSpec, sender: ComponentSender<Self>) {
        if let Some(cancel) = self.asset_watch_cancel.take() {
            cancel.cancel();
        }

        let paths = active_paths(&spec);
        if paths.is_empty() {
            tracing::debug!("no wallpaper asset paths to watch");
            return;
        }

        tracing::debug!(
            paths = %paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
            "watching wallpaper asset paths"
        );
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let input_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            watch_paths(paths, input_sender, task_cancel).await;
        });
        self.asset_watch_cancel = Some(cancel);
    }
}

fn active_paths(spec: &ResolvedWallpaperSpec) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(image) = &spec.image {
        paths.push(image.path.clone());
    }
    if let ResolvedBackdropSpec::Enabled {
        path: Some(path), ..
    } = &spec.backdrop
    {
        paths.push(path.clone());
    }
    paths.sort();
    paths.dedup();
    paths
}

async fn watch_paths(
    paths: Vec<PathBuf>,
    sender: relm4::Sender<AppCommand>,
    cancel: CancellationToken,
) {
    let watched: HashSet<PathBuf> = paths
        .iter()
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect();
    if watched.is_empty() {
        cancel.cancelled().await;
        return;
    }

    let watched_paths = paths.clone();
    let mut debouncer = match new_debouncer(
        Duration::from_millis(200),
        None,
        move |res: DebounceEventResult| {
            let Ok(events) = res else {
                return;
            };
            let touched = events.iter().any(|event| {
                let relevant_kind = matches!(
                    event.kind,
                    notify::EventKind::Create(_)
                        | notify::EventKind::Modify(_)
                        | notify::EventKind::Remove(_)
                );
                relevant_kind && event.paths.iter().any(|path| watched_paths.contains(path))
            });
            if touched {
                let _ = sender.send(AppCommand::ReloadAssets);
            }
        },
    ) {
        Ok(debouncer) => debouncer,
        Err(err) => {
            tracing::warn!("failed to create wallpaper asset watcher: {err}");
            return;
        }
    };

    for dir in watched {
        if let Err(err) = debouncer.watch(&dir, notify::RecursiveMode::NonRecursive) {
            tracing::warn!(
                "failed to watch wallpaper asset directory {}: {err}",
                dir.display()
            );
        } else {
            tracing::debug!(directory = %dir.display(), "watching wallpaper asset directory");
        }
    }

    tracing::debug!("wallpaper asset watcher is running");
    cancel.cancelled().await;
    tracing::debug!("wallpaper asset watcher stopped");
}

fn list_monitors(display: &gdk::Display) -> Vec<gdk::Monitor> {
    let monitors = display.monitors();
    let mut result = Vec::new();
    for index in 0..monitors.n_items() {
        let Some(obj) = monitors.item(index) else {
            continue;
        };
        let Ok(monitor) = obj.downcast::<gdk::Monitor>() else {
            continue;
        };
        result.push(monitor);
    }
    result
}

fn connector_name(monitor: &gdk::Monitor) -> String {
    monitor
        .connector()
        .map(|connector| connector.to_string())
        .unwrap_or_else(|| format!("{:?}", monitor.geometry()))
}

fn monitor_target_size(monitor: &gdk::Monitor) -> (i32, i32) {
    let geometry = monitor.geometry();
    let scale = monitor.scale_factor().max(1);
    (
        geometry.width().saturating_mul(scale),
        geometry.height().saturating_mul(scale),
    )
}

fn backdrop_label(backdrop: &ResolvedBackdropSpec) -> &'static str {
    match backdrop {
        ResolvedBackdropSpec::Disabled => "disabled",
        ResolvedBackdropSpec::Enabled { path: Some(_), .. } => "image",
        ResolvedBackdropSpec::Enabled { path: None, .. } => "enabled-no-image",
    }
}

#[derive(Debug)]
pub struct WallpaperWindowInit {
    monitor: gdk::Monitor,
    spec: ResolvedWallpaperSpec,
}

pub struct WallpaperWindow {
    color: Controller<ColorLayer>,
    image: Controller<ImageLayer>,
    target_size: (i32, i32),
}

#[derive(Debug)]
pub enum WallpaperWindowInput {
    Reconfigure {
        spec: ResolvedWallpaperSpec,
        target_size: (i32, i32),
        force_image_reload: bool,
    },
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for WallpaperWindow {
    type Init = WallpaperWindowInit;
    type Input = WallpaperWindowInput;
    type Output = ();

    view! {
        gtk::Window {
            set_decorated: false,

            #[name(overlay)]
            gtk::Overlay {}
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        setup_layer_window(&root, &init.monitor, WALLPAPER_NAMESPACE);
        tracing::info!(
            monitor = %connector_name(&init.monitor),
            color = %init.spec.color,
            image = init.spec.image.as_ref().map(|image| image.path.display().to_string()).as_deref().unwrap_or("<none>"),
            "presenting wallpaper surface"
        );
        let target_size = monitor_target_size(&init.monitor);

        let color = ColorLayer::builder()
            .launch(init.spec.color.clone())
            .detach();
        let image = ImageLayer::builder()
            .launch(ImageLayerInit {
                image: init.spec.image.clone(),
                transition_ms: init.spec.transition_ms,
                blur_radius: None,
                target_size: Some(target_size),
            })
            .detach();

        let color_widget = color.widget().clone().upcast::<gtk::Widget>();
        let image_widget = image.widget().clone().upcast::<gtk::Widget>();
        let widgets = view_output!();
        widgets.overlay.set_child(Some(&color_widget));
        widgets.overlay.add_overlay(&image_widget);
        root.present();

        let model = WallpaperWindow {
            color,
            image,
            target_size,
        };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            WallpaperWindowInput::Reconfigure {
                spec,
                target_size,
                force_image_reload,
            } => {
                self.target_size = target_size;
                self.color.emit(ColorLayerInput::SetColor(spec.color));
                self.image.emit(ImageLayerInput::Reconfigure {
                    init: ImageLayerInit {
                        image: spec.image,
                        transition_ms: spec.transition_ms,
                        blur_radius: None,
                        target_size: Some(self.target_size),
                    },
                    force_reload: force_image_reload,
                });
            }
        }
    }
}

#[derive(Debug)]
pub struct BackdropWindowInit {
    monitor: gdk::Monitor,
    backdrop: ResolvedBackdropSpec,
}

pub struct BackdropWindow {
    image: Controller<ImageLayer>,
    target_size: (i32, i32),
}

#[derive(Debug)]
pub enum BackdropWindowInput {
    Reconfigure {
        backdrop: ResolvedBackdropSpec,
        target_size: (i32, i32),
        force_image_reload: bool,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for BackdropWindow {
    type Init = BackdropWindowInit;
    type Input = BackdropWindowInput;
    type Output = ();

    view! {
        gtk::Window {
            set_decorated: false,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        setup_layer_window(&root, &init.monitor, BACKDROP_NAMESPACE);
        let target_size = monitor_target_size(&init.monitor);
        tracing::info!(
            monitor = %connector_name(&init.monitor),
            target_width = target_size.0,
            target_height = target_size.1,
            backdrop = backdrop_label(&init.backdrop),
            "presenting backdrop surface"
        );
        let image = ImageLayer::builder()
            .launch(backdrop_image_init(init.backdrop, Some(target_size)))
            .detach();
        root.set_child(Some(image.widget()));
        root.present();

        let widgets = view_output!();
        let model = BackdropWindow { image, target_size };
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BackdropWindowInput::Reconfigure {
                backdrop,
                target_size,
                force_image_reload,
            } => {
                self.target_size = target_size;
                self.image.emit(ImageLayerInput::Reconfigure {
                    init: backdrop_image_init(backdrop, Some(self.target_size)),
                    force_reload: force_image_reload,
                });
            }
        }
    }
}

fn backdrop_image_init(
    backdrop: ResolvedBackdropSpec,
    target_size: Option<(i32, i32)>,
) -> ImageLayerInit {
    match backdrop {
        ResolvedBackdropSpec::Disabled => ImageLayerInit {
            image: None,
            transition_ms: 0,
            blur_radius: None,
            target_size,
        },
        ResolvedBackdropSpec::Enabled { path, blur_radius } => ImageLayerInit {
            image: path.map(|path| ResolvedImageSpec {
                path,
                fit: FitMode::Cover,
            }),
            transition_ms: 0,
            blur_radius: Some(blur_radius),
            target_size,
        },
    }
}

fn setup_layer_window(window: &gtk::Window, monitor: &gdk::Monitor, namespace: &str) {
    window.init_layer_shell();
    window.set_layer(Layer::Background);
    window.set_namespace(Some(namespace));
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_exclusive_zone(-1);
    window.set_monitor(Some(monitor));
    window.set_decorated(false);
    window.set_deletable(false);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
}

struct ColorLayer {
    area: gtk::DrawingArea,
}

#[derive(Debug)]
enum ColorLayerInput {
    SetColor(String),
}

#[relm4::component]
impl SimpleComponent for ColorLayer {
    type Init = String;
    type Input = ColorLayerInput;
    type Output = ();

    view! {
        gtk::DrawingArea {
            set_hexpand: true,
            set_vexpand: true,
        }
    }

    fn init(
        color: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ColorLayer { area: root.clone() };
        let widgets = view_output!();
        let _ = sender.input(ColorLayerInput::SetColor(color));
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            ColorLayerInput::SetColor(color) => apply_color(&self.area, &color),
        }
    }
}

fn apply_color(area: &gtk::DrawingArea, color: &str) {
    if let Ok(Srgb {
        red,
        green,
        blue,
        alpha,
    }) = color.parse::<Srgb>()
    {
        area.set_draw_func(move |_, cr, _, _| {
            cr.set_source_rgba(red as f64, green as f64, blue as f64, alpha as f64);
            let _ = cr.paint();
        });
        area.queue_draw();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImageLayerInit {
    image: Option<ResolvedImageSpec>,
    transition_ms: u32,
    blur_radius: Option<u32>,
    target_size: Option<(i32, i32)>,
}

struct ImageLayer {
    request_id: u64,
    current: ImageLayerInit,
    active_slot: PictureSlot,
    has_paintable: bool,
    front_picture: gtk::Picture,
    back_picture: gtk::Picture,
    pending_clear: Option<glib::SourceId>,
}

#[derive(Debug)]
enum ImageLayerInput {
    Reconfigure {
        init: ImageLayerInit,
        force_reload: bool,
    },
    Loaded {
        request_id: u64,
        result: Result<DecodedImage, String>,
    },
    PreviewLoaded {
        request_id: u64,
        result: Result<Option<DecodedImage>, String>,
    },
    ClearHidden {
        request_id: u64,
        slot: PictureSlot,
    },
}

struct DecodedImage {
    width: i32,
    height: i32,
    stride: usize,
    pixels: Vec<u8>,
}

impl std::fmt::Debug for DecodedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecodedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("stride", &self.stride)
            .field("pixel_bytes", &self.pixels.len())
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PictureSlot {
    Front,
    Back,
}

#[allow(unused_assignments)]
#[relm4::component]
impl Component for ImageLayer {
    type Init = ImageLayerInit;
    type Input = ImageLayerInput;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Stack {
            set_hexpand: true,
            set_vexpand: true,
            set_halign: gtk::Align::Fill,
            set_valign: gtk::Align::Fill,
            set_transition_type: gtk::StackTransitionType::Crossfade,

            #[local_ref]
            front_picture -> gtk::Picture {
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
                set_can_shrink: true,
                set_content_fit: content_fit(&init.image),
            },

            #[local_ref]
            back_picture -> gtk::Picture {
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
                set_can_shrink: true,
                set_content_fit: content_fit(&init.image),
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let front_picture = gtk::Picture::new();
        let back_picture = gtk::Picture::new();
        let widgets = view_output!();
        root.set_transition_duration(init.transition_ms);
        root.set_visible(false);
        root.set_visible_child(&back_picture);

        let model = ImageLayer {
            request_id: 0,
            current: init.clone(),
            active_slot: PictureSlot::Back,
            has_paintable: false,
            front_picture,
            back_picture,
            pending_clear: None,
        };
        let _ = sender.input(ImageLayerInput::Reconfigure {
            init,
            force_reload: false,
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            ImageLayerInput::Reconfigure {
                init: next,
                force_reload,
            } => {
                let fit_changed = content_fit(&self.current.image) != content_fit(&next.image);
                let transition_changed = self.current.transition_ms != next.transition_ms;
                let start_image_load =
                    should_start_image_load(&self.current, &next, force_reload, self.request_id);
                self.current = next.clone();

                if fit_changed {
                    self.front_picture.set_content_fit(content_fit(&next.image));
                    self.back_picture.set_content_fit(content_fit(&next.image));
                }
                if transition_changed {
                    root.set_transition_duration(next.transition_ms);
                }
                root.set_visible(next.image.is_some() && self.has_paintable);
                if next.image.is_none() {
                    self.request_id += 1;
                    self.front_picture.set_paintable(None::<&gdk::Paintable>);
                    self.back_picture.set_paintable(None::<&gdk::Paintable>);
                    self.has_paintable = false;
                    return;
                }
                if start_image_load {
                    self.request_id += 1;
                    let image = next.image.expect("image presence checked above");
                    spawn_image_load(
                        self.request_id,
                        image.path,
                        image.fit,
                        next.blur_radius,
                        next.target_size,
                        sender.input_sender().clone(),
                    );
                }
            }
            ImageLayerInput::Loaded { request_id, result } => {
                if request_id != self.request_id {
                    tracing::debug!(
                        request_id,
                        active_request_id = self.request_id,
                        "ignoring stale wallpaper image load"
                    );
                    return;
                }
                match result {
                    Ok(decoded) => {
                        tracing::info!(
                            request_id,
                            width = decoded.width,
                            height = decoded.height,
                            stride = decoded.stride,
                            "applying decoded wallpaper texture"
                        );
                        let next_slot = hidden_slot(self.active_slot);
                        let picture = self.picture_for_slot(next_slot);
                        picture.set_paintable(Some(&decoded.into_texture()));
                        root.set_visible(true);
                        root.set_visible_child(picture);
                        let previous_slot = self.active_slot;
                        self.active_slot = next_slot;
                        self.has_paintable = true;
                        if let Some(source) = self.pending_clear.take() {
                            source.remove();
                        }
                        self.pending_clear = Some(schedule_hidden_slot_clear(
                            self.request_id,
                            previous_slot,
                            self.current.transition_ms,
                            sender.input_sender().clone(),
                        ));
                    }
                    Err(error) => tracing::warn!("failed to load wallpaper image: {error}"),
                }
            }
            ImageLayerInput::PreviewLoaded { request_id, result } => {
                if request_id != self.request_id || self.has_paintable {
                    return;
                }
                match result {
                    Ok(Some(decoded)) => {
                        tracing::debug!(
                            request_id,
                            width = decoded.width,
                            height = decoded.height,
                            stride = decoded.stride,
                            "applying wallpaper preview texture"
                        );
                        let next_slot = hidden_slot(self.active_slot);
                        let picture = self.picture_for_slot(next_slot);
                        picture.set_paintable(Some(&decoded.into_texture()));
                        root.set_visible(true);
                        root.set_visible_child(picture);
                        self.active_slot = next_slot;
                        self.has_paintable = true;
                    }
                    Ok(None) => {}
                    Err(error) => tracing::debug!("failed to load wallpaper preview: {error}"),
                }
            }
            ImageLayerInput::ClearHidden { request_id, slot } => {
                if request_id == self.request_id {
                    self.pending_clear = None;
                    if slot != self.active_slot {
                        self.picture_for_slot(slot)
                            .set_paintable(None::<&gdk::Paintable>);
                    }
                }
            }
        }
    }
}

impl ImageLayer {
    fn picture_for_slot(&self, slot: PictureSlot) -> &gtk::Picture {
        match slot {
            PictureSlot::Front => &self.front_picture,
            PictureSlot::Back => &self.back_picture,
        }
    }
}

fn hidden_slot(active: PictureSlot) -> PictureSlot {
    match active {
        PictureSlot::Front => PictureSlot::Back,
        PictureSlot::Back => PictureSlot::Front,
    }
}

fn content_fit(image: &Option<ResolvedImageSpec>) -> ContentFit {
    match image
        .as_ref()
        .map(|image| image.fit)
        .unwrap_or(FitMode::Cover)
    {
        FitMode::Cover => ContentFit::Cover,
        FitMode::Contain => ContentFit::Contain,
        FitMode::Fill => ContentFit::Fill,
    }
}

fn should_start_image_load(
    current: &ImageLayerInit,
    next: &ImageLayerInit,
    force_reload: bool,
    request_id: u64,
) -> bool {
    next.image.is_some()
        && (force_reload
            || request_id == 0
            || current.image != next.image
            || current.blur_radius != next.blur_radius
            || current.target_size != next.target_size)
}

fn spawn_image_load(
    request_id: u64,
    path: PathBuf,
    fit: FitMode,
    blur_radius: Option<u32>,
    target_size: Option<(i32, i32)>,
    sender: relm4::Sender<ImageLayerInput>,
) {
    relm4::spawn(async move {
        let path_for_log = path.clone();
        tracing::info!(
            request_id,
            path = %path_for_log.display(),
            blur_radius = blur_radius.unwrap_or_default(),
            target_width = target_size.map(|(width, _)| width).unwrap_or_default(),
            target_height = target_size.map(|(_, height)| height).unwrap_or_default(),
            "loading wallpaper image"
        );

        let preview_task = should_load_heic_preview(&path, blur_radius).then(|| {
            let preview_path = path.clone();
            let preview_sender = sender.clone();
            tokio::spawn(async move {
                let preview =
                    tokio::task::spawn_blocking(move || decode_heic_preview(&preview_path))
                        .await
                        .map_err(|error| format!("wallpaper preview worker failed: {error}"))
                        .and_then(|result| result.map_err(|error| error.to_string()));
                let _ = preview_sender.send(ImageLayerInput::PreviewLoaded {
                    request_id,
                    result: preview,
                });
            })
        });

        let result =
            tokio::task::spawn_blocking(move || decode_image(&path, fit, blur_radius, target_size))
                .await
                .map_err(|error| format!("wallpaper worker failed: {error}"))
                .and_then(|result| result.map_err(|error| error.to_string()));
        match &result {
            Ok(decoded) => tracing::info!(
                request_id,
                path = %path_for_log.display(),
                width = decoded.width,
                height = decoded.height,
                stride = decoded.stride,
                "wallpaper image decoded and converted"
            ),
            Err(error) => {
                tracing::warn!(
                    request_id,
                    path = %path_for_log.display(),
                    "wallpaper image decode failed: {error}"
                );
            }
        }
        let _ = sender.send(ImageLayerInput::Loaded { request_id, result });
        if let Some(handle) = preview_task {
            let _ = handle.await;
        }
    });
}

fn should_load_heic_preview(path: &Path, blur_radius: Option<u32>) -> bool {
    blur_radius.unwrap_or_default() == 0 && heic::is_heic_path(path)
}

fn decode_heic_preview(path: &Path) -> anyhow::Result<Option<DecodedImage>> {
    if let Some(preview) = load_legacy_unprocessed_preview_cache(path)? {
        return Ok(Some(preview));
    }

    heic::decode_thumbnail(path).map(|decoded| {
        decoded.map(|decoded| DecodedImage {
            width: decoded.width,
            height: decoded.height,
            stride: decoded.stride,
            pixels: decoded.pixels,
        })
    })
}

fn load_legacy_unprocessed_preview_cache(path: &Path) -> anyhow::Result<Option<DecodedImage>> {
    let Some(legacy_key) = ImageCacheKey::new_legacy(path, None, None)? else {
        return Ok(None);
    };
    let preview = load_cached_image(&legacy_key)?;
    if let Some(preview) = &preview {
        tracing::info!(
            path = %path.display(),
            cache_path = %legacy_key.path.display(),
            width = preview.width,
            height = preview.height,
            stride = preview.stride,
            "loaded legacy wallpaper preview cache"
        );
    }
    Ok(preview)
}

fn schedule_hidden_slot_clear(
    request_id: u64,
    slot: PictureSlot,
    transition_ms: u32,
    sender: relm4::Sender<ImageLayerInput>,
) -> glib::SourceId {
    let delay = Duration::from_millis(transition_ms as u64 + 50);
    glib::timeout_add_local_once(delay, move || {
        let _ = sender.send(ImageLayerInput::ClearHidden { request_id, slot });
    })
}

fn decode_image(
    path: &Path,
    fit: FitMode,
    blur_radius: Option<u32>,
    target_size: Option<(i32, i32)>,
) -> anyhow::Result<DecodedImage> {
    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }

    let cache_key = ImageCacheKey::new(path, fit, blur_radius, target_size)?;
    if let Some(cache_key) = &cache_key {
        if let Some(cached) = load_cached_image(cache_key)? {
            tracing::info!(
                path = %path.display(),
                cache_path = %cache_key.path.display(),
                width = cached.width,
                height = cached.height,
                stride = cached.stride,
                pixel_bytes = cached.pixels.len(),
                "loaded wallpaper image from cache"
            );
            return Ok(cached);
        }
        if let Some(migrated) = load_legacy_unprocessed_cache(path, fit, target_size, cache_key)? {
            return Ok(migrated);
        }
    }

    tracing::debug!(path = %path.display(), "decoding wallpaper image file");
    let mut image = if heic::is_heic_path(path) {
        tracing::debug!(path = %path.display(), "decoding HEIC/HEIF wallpaper with libheif");
        let decoded = heic::decode(path)?;
        tracing::debug!(
            path = %path.display(),
            width = decoded.width,
            height = decoded.height,
            stride = decoded.stride,
            "converting HEIC/HEIF wallpaper to RGBA8"
        );
        decoded.into_rgba_image()
    } else {
        let image = image::open(path)?;
        let source_format = image_color_label(image.color());
        let source_width = image.width();
        let source_height = image.height();
        tracing::debug!(
            path = %path.display(),
            width = source_width,
            height = source_height,
            source_format,
            "converting wallpaper image to RGBA8"
        );
        image.into_rgba8()
    };
    if let Some(radius) = blur_radius.filter(|radius| *radius > 0) {
        if let Some((target_width, target_height)) = target_size {
            let target_width = target_width.max(1) as u32;
            let target_height = target_height.max(1) as u32;
            let (texture_width, texture_height) =
                backdrop_texture_dimensions(target_width, target_height);
            let (work_width, work_height, work_blur_radius) =
                blur_processing_dimensions(texture_width, texture_height, radius);
            tracing::debug!(
                path = %path.display(),
                source_width = image.width(),
                source_height = image.height(),
                work_width,
                work_height,
                texture_width,
                texture_height,
                target_width,
                target_height,
                blur_radius = radius,
                work_blur_radius,
                "resizing backdrop before blur"
            );
            image = resize_rgba_to_cover(image, work_width, work_height);
            image = image::imageops::blur(&image, work_blur_radius as f32);
            if (work_width, work_height) != (texture_width, texture_height) {
                image = resize_rgba_to_cover(image, texture_width, texture_height);
            }
        } else {
            tracing::debug!(
                path = %path.display(),
                blur_radius = radius,
                "applying backdrop blur during image conversion"
            );
            image = image::imageops::blur(&image, radius as f32);
        }
    } else if let Some((target_width, target_height)) = target_size {
        let target_width = target_width.max(1) as u32;
        let target_height = target_height.max(1) as u32;
        tracing::debug!(
            path = %path.display(),
            source_width = image.width(),
            source_height = image.height(),
            target_width,
            target_height,
            fit = ?fit,
            "resizing wallpaper to output-sized texture"
        );
        image = resize_rgba_for_fit(image, target_width, target_height, fit);
    }
    let (width, height) = image.dimensions();
    let decoded = DecodedImage {
        width: width as i32,
        height: height as i32,
        stride: (width * 4) as usize,
        pixels: image.into_raw(),
    };
    if let Some(cache_key) = &cache_key {
        if let Err(error) = write_cached_image(cache_key, &decoded) {
            tracing::warn!(
                path = %path.display(),
                cache_path = %cache_key.path.display(),
                "failed to update wallpaper image cache: {error}"
            );
        } else {
            tracing::debug!(
                path = %path.display(),
                cache_path = %cache_key.path.display(),
                width = decoded.width,
                height = decoded.height,
                pixel_bytes = decoded.pixels.len(),
                "updated wallpaper image cache"
            );
        }
    }
    Ok(decoded)
}

fn resize_rgba_to_cover(image: image::RgbaImage, width: u32, height: u32) -> image::RgbaImage {
    image::DynamicImage::ImageRgba8(image)
        .resize_to_fill(
            width.max(1),
            height.max(1),
            image::imageops::FilterType::Lanczos3,
        )
        .into_rgba8()
}

fn resize_rgba_for_fit(
    image: image::RgbaImage,
    width: u32,
    height: u32,
    fit: FitMode,
) -> image::RgbaImage {
    let width = width.max(1);
    let height = height.max(1);
    let image = image::DynamicImage::ImageRgba8(image);
    match fit {
        FitMode::Cover => image
            .resize_to_fill(width, height, image::imageops::FilterType::Lanczos3)
            .into_rgba8(),
        FitMode::Contain => image
            .resize(width, height, image::imageops::FilterType::Lanczos3)
            .into_rgba8(),
        FitMode::Fill => image
            .resize_exact(width, height, image::imageops::FilterType::Lanczos3)
            .into_rgba8(),
    }
}

fn load_legacy_unprocessed_cache(
    path: &Path,
    fit: FitMode,
    target_size: Option<(i32, i32)>,
    current_cache_key: &ImageCacheKey,
) -> anyhow::Result<Option<DecodedImage>> {
    let Some((target_width, target_height)) = target_size else {
        return Ok(None);
    };
    let Some(legacy_key) = ImageCacheKey::new_legacy(path, None, None)? else {
        return Ok(None);
    };
    if legacy_key.path == current_cache_key.path {
        return Ok(None);
    }
    let Some(legacy) = load_cached_image(&legacy_key)? else {
        return Ok(None);
    };

    tracing::info!(
        path = %path.display(),
        cache_path = %legacy_key.path.display(),
        width = legacy.width,
        height = legacy.height,
        target_width,
        target_height,
        fit = ?fit,
        "resizing legacy wallpaper cache for output-sized texture"
    );
    let image = legacy.into_rgba_image()?;
    let image = resize_rgba_for_fit(
        image,
        target_width.max(1) as u32,
        target_height.max(1) as u32,
        fit,
    );
    let (width, height) = image.dimensions();
    let decoded = DecodedImage {
        width: width as i32,
        height: height as i32,
        stride: (width * 4) as usize,
        pixels: image.into_raw(),
    };
    if let Err(error) = write_cached_image(current_cache_key, &decoded) {
        tracing::warn!(
            path = %path.display(),
            cache_path = %current_cache_key.path.display(),
            "failed to update wallpaper image cache from legacy cache: {error}"
        );
    }
    Ok(Some(decoded))
}

fn backdrop_texture_dimensions(width: u32, height: u32) -> (u32, u32) {
    let scale = (1920.0 / width as f32).min(1080.0 / height as f32).min(1.0);
    (
        ((width as f32 * scale).round() as u32).max(1),
        ((height as f32 * scale).round() as u32).max(1),
    )
}

fn blur_processing_dimensions(width: u32, height: u32, blur_radius: u32) -> (u32, u32, u32) {
    let divisor = (blur_radius / 8).clamp(1, 4);
    let work_width = (width / divisor).max(1);
    let work_height = (height / divisor).max(1);
    let work_blur_radius = (blur_radius / divisor).max(1);
    (work_width, work_height, work_blur_radius)
}

struct ImageCacheKey {
    path: PathBuf,
}

impl ImageCacheKey {
    fn new(
        source_path: &Path,
        fit: FitMode,
        blur_radius: Option<u32>,
        target_size: Option<(i32, i32)>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(cache_root) = dirs::cache_dir().map(|dir| dir.join("glimpse").join("wallpaper"))
        else {
            return Ok(None);
        };
        let signature = source_signature(source_path)?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        "glimpse-wallpaper-rgba-v2".hash(&mut hasher);
        source_path.hash(&mut hasher);
        signature.hash(&mut hasher);
        fit.hash(&mut hasher);
        blur_radius.unwrap_or_default().hash(&mut hasher);
        normalized_target_size(target_size).hash(&mut hasher);
        let digest = hasher.finish();
        Ok(Some(Self {
            path: cache_root.join(format!("{digest:016x}.rgba")),
        }))
    }

    fn new_legacy(
        source_path: &Path,
        blur_radius: Option<u32>,
        target_size: Option<(i32, i32)>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(cache_root) = dirs::cache_dir().map(|dir| dir.join("glimpse").join("wallpaper"))
        else {
            return Ok(None);
        };
        let signature = source_signature(source_path)?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        "glimpse-wallpaper-rgba-v1".hash(&mut hasher);
        source_path.hash(&mut hasher);
        signature.hash(&mut hasher);
        blur_radius.unwrap_or_default().hash(&mut hasher);
        normalized_target_size(target_size).hash(&mut hasher);
        let digest = hasher.finish();
        Ok(Some(Self {
            path: cache_root.join(format!("{digest:016x}.rgba")),
        }))
    }
}

fn normalized_target_size(target_size: Option<(i32, i32)>) -> (i32, i32) {
    target_size
        .map(|(width, height)| (width.max(1), height.max(1)))
        .unwrap_or((0, 0))
}

fn source_signature(source_path: &Path) -> anyhow::Result<String> {
    let metadata = fs::metadata(source_path)?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| (duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or((0, 0));

    Ok(format!("{}:{}:{}", metadata.len(), modified.0, modified.1))
}

fn load_cached_image(cache_key: &ImageCacheKey) -> anyhow::Result<Option<DecodedImage>> {
    let bytes = match fs::read(&cache_key.path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Some(header_end) = bytes.iter().position(|byte| *byte == b'\n') else {
        tracing::warn!(
            cache_path = %cache_key.path.display(),
            "ignoring wallpaper image cache with missing header"
        );
        return Ok(None);
    };
    let header = std::str::from_utf8(&bytes[..header_end])?;
    let mut fields = header.split(' ');
    let Some("GLIMPSE_RGBA_V1") = fields.next() else {
        tracing::warn!(
            cache_path = %cache_key.path.display(),
            "ignoring wallpaper image cache with invalid magic"
        );
        return Ok(None);
    };
    let width = parse_cache_i32(fields.next(), "width")?;
    let height = parse_cache_i32(fields.next(), "height")?;
    let stride = parse_cache_usize(fields.next(), "stride")?;
    if fields.next().is_some() || width <= 0 || height <= 0 || stride == 0 {
        tracing::warn!(
            cache_path = %cache_key.path.display(),
            "ignoring wallpaper image cache with invalid dimensions"
        );
        return Ok(None);
    }
    let pixels = bytes[header_end + 1..].to_vec();
    let expected_len = stride.saturating_mul(height as usize);
    if pixels.len() != expected_len {
        tracing::warn!(
            cache_path = %cache_key.path.display(),
            pixel_bytes = pixels.len(),
            expected_pixel_bytes = expected_len,
            "ignoring wallpaper image cache with invalid pixel length"
        );
        return Ok(None);
    }

    Ok(Some(DecodedImage {
        width,
        height,
        stride,
        pixels,
    }))
}

fn parse_cache_i32(value: Option<&str>, field: &str) -> anyhow::Result<i32> {
    value
        .ok_or_else(|| anyhow::anyhow!("missing cached image {field}"))?
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid cached image {field}: {error}"))
}

fn parse_cache_usize(value: Option<&str>, field: &str) -> anyhow::Result<usize> {
    value
        .ok_or_else(|| anyhow::anyhow!("missing cached image {field}"))?
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid cached image {field}: {error}"))
}

fn write_cached_image(cache_key: &ImageCacheKey, image: &DecodedImage) -> anyhow::Result<()> {
    if let Some(parent) = cache_key.path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = format!(
        "GLIMPSE_RGBA_V1 {} {} {}\n",
        image.width, image.height, image.stride
    )
    .into_bytes();
    bytes.extend_from_slice(&image.pixels);
    fs::write(&cache_key.path, bytes)?;
    Ok(())
}

impl DecodedImage {
    fn into_rgba_image(self) -> anyhow::Result<image::RgbaImage> {
        image::RgbaImage::from_raw(self.width as u32, self.height as u32, self.pixels)
            .ok_or_else(|| anyhow::anyhow!("cached image dimensions do not match pixel buffer"))
    }

    fn into_texture(self) -> gdk::MemoryTexture {
        let bytes = glib::Bytes::from_owned(self.pixels);
        gdk::MemoryTexture::new(
            self.width,
            self.height,
            gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            self.stride,
        )
    }
}

fn image_color_label(color: image::ColorType) -> &'static str {
    match color {
        image::ColorType::L8 => "luma8",
        image::ColorType::La8 => "luma-alpha8",
        image::ColorType::Rgb8 => "rgb8",
        image::ColorType::Rgba8 => "rgba8",
        image::ColorType::L16 => "luma16",
        image::ColorType::La16 => "luma-alpha16",
        image::ColorType::Rgb16 => "rgb16",
        image::ColorType::Rgba16 => "rgba16",
        image::ColorType::Rgb32F => "rgb32f",
        image::ColorType::Rgba32F => "rgba32f",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DecodedImage, ImageCacheKey, ImageLayerInit, active_paths, backdrop_texture_dimensions,
        blur_processing_dimensions, load_cached_image, load_legacy_unprocessed_cache,
        resize_rgba_for_fit, should_start_image_load, write_cached_image,
    };
    use glimpse_core::{FitMode, ResolvedBackdropSpec, ResolvedImageSpec, ResolvedWallpaperSpec};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn decoded_image_debug_does_not_include_pixel_buffer() {
        let decoded = DecodedImage {
            width: 1,
            height: 1,
            stride: 4,
            pixels: vec![1, 2, 3, 4],
        };

        let debug = format!("{decoded:?}");

        assert!(debug.contains("width: 1"));
        assert!(debug.contains("height: 1"));
        assert!(debug.contains("stride: 4"));
        assert!(debug.contains("pixel_bytes: 4"));
        assert!(!debug.contains("pixels"));
        assert!(!debug.contains("[1, 2, 3, 4]"));
    }

    #[test]
    fn force_reload_starts_image_load_even_when_path_is_unchanged() {
        let init = ImageLayerInit {
            image: Some(ResolvedImageSpec {
                path: PathBuf::from("/tmp/wallpaper.png"),
                fit: FitMode::Cover,
            }),
            transition_ms: 800,
            blur_radius: None,
            target_size: None,
        };

        assert!(should_start_image_load(&init, &init, true, 7));
    }

    #[test]
    fn unchanged_image_without_force_does_not_start_image_load_after_initial_load() {
        let init = ImageLayerInit {
            image: Some(ResolvedImageSpec {
                path: PathBuf::from("/tmp/wallpaper.png"),
                fit: FitMode::Cover,
            }),
            transition_ms: 800,
            blur_radius: None,
            target_size: None,
        };

        assert!(!should_start_image_load(&init, &init, false, 7));
    }

    #[test]
    fn backdrop_blur_processing_caps_to_1080p() {
        assert_eq!(backdrop_texture_dimensions(3840, 2160), (1920, 1080));
        assert_eq!(backdrop_texture_dimensions(3072, 1728), (1920, 1080));
        assert_eq!(backdrop_texture_dimensions(1280, 720), (1280, 720));
    }

    #[test]
    fn backdrop_blur_processing_downsamples_large_blurs() {
        assert_eq!(blur_processing_dimensions(1920, 1080, 24), (640, 360, 8));
        assert_eq!(blur_processing_dimensions(1280, 720, 24), (426, 240, 8));
        assert_eq!(blur_processing_dimensions(1280, 720, 4), (1280, 720, 4));
    }

    #[test]
    fn cover_resize_produces_exact_output_size() {
        let image = image::RgbaImage::new(400, 200);
        let resized = resize_rgba_for_fit(image, 100, 100, FitMode::Cover);

        assert_eq!(resized.dimensions(), (100, 100));
    }

    #[test]
    fn contain_resize_preserves_aspect_inside_output_size() {
        let image = image::RgbaImage::new(400, 200);
        let resized = resize_rgba_for_fit(image, 100, 100, FitMode::Contain);

        assert_eq!(resized.dimensions(), (100, 50));
    }

    #[test]
    fn fill_resize_produces_exact_output_size() {
        let image = image::RgbaImage::new(400, 200);
        let resized = resize_rgba_for_fit(image, 100, 100, FitMode::Fill);

        assert_eq!(resized.dimensions(), (100, 100));
    }

    #[test]
    fn decoded_image_cache_round_trips_raw_pixels() {
        let cache_dir = temp_path("image-cache");
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_key = ImageCacheKey {
            path: cache_dir.join("entry.rgba"),
        };
        let decoded = DecodedImage {
            width: 2,
            height: 1,
            stride: 8,
            pixels: vec![1, 2, 3, 4, 5, 6, 7, 8],
        };

        write_cached_image(&cache_key, &decoded).unwrap();
        let cached = load_cached_image(&cache_key).unwrap().unwrap();

        assert_eq!(cached.width, 2);
        assert_eq!(cached.height, 1);
        assert_eq!(cached.stride, 8);
        assert_eq!(cached.pixels, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn legacy_unprocessed_cache_seeds_target_sized_cache() {
        let source = temp_path("source.heic");
        fs::write(&source, b"fake image bytes").unwrap();
        let current_key = ImageCacheKey::new(&source, FitMode::Cover, None, Some((100, 100)))
            .unwrap()
            .unwrap();
        let legacy_key = ImageCacheKey::new_legacy(&source, None, None)
            .unwrap()
            .unwrap();
        let legacy = DecodedImage {
            width: 400,
            height: 200,
            stride: 1600,
            pixels: vec![255; 400 * 200 * 4],
        };
        write_cached_image(&legacy_key, &legacy).unwrap();

        let migrated =
            load_legacy_unprocessed_cache(&source, FitMode::Cover, Some((100, 100)), &current_key)
                .unwrap()
                .unwrap();
        let current = load_cached_image(&current_key).unwrap().unwrap();

        assert_eq!((migrated.width, migrated.height), (100, 100));
        assert_eq!((current.width, current.height), (100, 100));

        let _ = fs::remove_file(source);
        let _ = fs::remove_file(current_key.path);
        let _ = fs::remove_file(legacy_key.path);
    }

    #[test]
    fn active_paths_dedupes_wallpaper_and_backdrop_path() {
        let path = PathBuf::from("/tmp/wallpaper.png");
        let spec = ResolvedWallpaperSpec {
            color: "#101010".into(),
            image: Some(ResolvedImageSpec {
                path: path.clone(),
                fit: FitMode::Cover,
            }),
            transition_ms: 800,
            backdrop: ResolvedBackdropSpec::Enabled {
                path: Some(path.clone()),
                blur_radius: 24,
            },
        };

        assert_eq!(active_paths(&spec), vec![path]);
    }

    fn temp_path(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("glimpse-wallpaper-{name}-{suffix}"))
    }
}
