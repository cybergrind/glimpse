use std::{
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    os::fd::AsFd,
    path::PathBuf,
    time::Duration,
};

use async_trait::async_trait;
use uuid::Uuid;
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    protocol::{wl_output, wl_registry},
};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1, zwlr_gamma_control_v1,
};

use crate::compositor::{CompositorCapabilities, CompositorKind};

const GAMMA_CONTROL_RETRY_DELAY: Duration = Duration::from_millis(150);
const GAMMA_CONTROL_RETRY_ATTEMPTS: usize = 8;

#[async_trait]
pub(crate) trait NightLightBackend: Send {
    fn compositor(&self) -> CompositorKind;
    fn compositor_capabilities(&self) -> CompositorCapabilities;
    async fn apply_temperature(&mut self, temperature_kelvin: u32) -> anyhow::Result<()>;
    async fn reset(&mut self) -> anyhow::Result<()>;
}

pub(crate) fn create_backend(compositor: CompositorKind) -> Box<dyn NightLightBackend> {
    match compositor {
        CompositorKind::Niri | CompositorKind::Hyprland => {
            Box::new(WaylandNightLightBackend::new(compositor))
        }
        CompositorKind::Unknown => Box::new(UnsupportedNightLightBackend::new(compositor)),
    }
}

pub(crate) struct UnsupportedNightLightBackend {
    compositor: CompositorKind,
}

impl UnsupportedNightLightBackend {
    pub fn new(compositor: CompositorKind) -> Self {
        Self { compositor }
    }
}

#[async_trait]
impl NightLightBackend for UnsupportedNightLightBackend {
    fn compositor(&self) -> CompositorKind {
        self.compositor
    }

    fn compositor_capabilities(&self) -> CompositorCapabilities {
        self.compositor.capabilities()
    }

    async fn apply_temperature(&mut self, _temperature_kelvin: u32) -> anyhow::Result<()> {
        anyhow::bail!(
            "night light backend is unavailable for {}",
            self.compositor.label()
        );
    }

    async fn reset(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub(crate) struct WaylandNightLightBackend {
    compositor: CompositorKind,
    controller: Option<WaylandGammaController>,
}

impl WaylandNightLightBackend {
    fn new(compositor: CompositorKind) -> Self {
        Self {
            compositor,
            controller: None,
        }
    }

    fn controller_mut(&mut self) -> anyhow::Result<&mut WaylandGammaController> {
        if self.controller.is_none() {
            self.controller = Some(WaylandGammaController::connect()?);
        }
        self.controller
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("failed to initialize wayland gamma controller"))
    }
}

#[async_trait]
impl NightLightBackend for WaylandNightLightBackend {
    fn compositor(&self) -> CompositorKind {
        self.compositor
    }

    fn compositor_capabilities(&self) -> CompositorCapabilities {
        self.compositor.capabilities()
    }

    async fn apply_temperature(&mut self, temperature_kelvin: u32) -> anyhow::Result<()> {
        let controller = self.controller_mut()?;
        controller.apply_temperature(temperature_kelvin)
    }

    async fn reset(&mut self) -> anyhow::Result<()> {
        if let Some(controller) = self.controller.as_mut() {
            controller.reset()?;
        }
        Ok(())
    }
}

struct WaylandGammaController {
    conn: Connection,
    event_queue: EventQueue<GammaState>,
    state: GammaState,
}

impl WaylandGammaController {
    fn connect() -> anyhow::Result<Self> {
        let conn = Connection::connect_to_env()?;
        let mut event_queue = conn.new_event_queue();
        let qh = event_queue.handle();
        conn.display().get_registry(&qh, ());

        let mut state = GammaState::default();
        event_queue.roundtrip(&mut state)?;

        if state.manager.is_none() {
            anyhow::bail!("wayland gamma-control protocol is unavailable");
        }
        if state.outputs.is_empty() {
            anyhow::bail!("no wayland outputs available for night light");
        }

        Ok(Self {
            conn,
            event_queue,
            state,
        })
    }

    fn apply_temperature(&mut self, temperature_kelvin: u32) -> anyhow::Result<()> {
        self.ensure_controls()?;

        let (red_scale, green_scale, blue_scale) = temperature_rgb_scales(temperature_kelvin);
        let mut applied_controls = 0usize;
        let mut gamma_files = Vec::new();

        for output_name in self.state.output_names() {
            let Some(entry) = self.state.controls.get(&output_name) else {
                continue;
            };
            let Some(gamma_size) = entry.gamma_size else {
                continue;
            };
            let file = write_gamma_ramp(gamma_size, red_scale, green_scale, blue_scale)?;
            entry.control.set_gamma(file.as_fd());
            gamma_files.push(file);
            applied_controls += 1;
        }

        if applied_controls == 0 {
            anyhow::bail!(
                "no usable outputs accepted gamma control for {}",
                self.state.output_label_list()
            );
        }

        self.conn.flush()?;
        self.event_queue.dispatch_pending(&mut self.state)?;
        self.state.prune_failed_controls();
        if self.state.controls.is_empty() {
            anyhow::bail!(
                "all gamma control objects failed for {}",
                self.state.output_label_list()
            );
        }

        Ok(())
    }

    fn reset(&mut self) -> anyhow::Result<()> {
        for (_, entry) in self.state.controls.drain() {
            entry.control.destroy();
        }
        self.conn.flush()?;
        self.event_queue.dispatch_pending(&mut self.state)?;
        Ok(())
    }

    fn ensure_controls(&mut self) -> anyhow::Result<()> {
        if self.state.manager.is_none() {
            anyhow::bail!("wayland gamma-control manager disappeared");
        }

        for attempt in 0..=GAMMA_CONTROL_RETRY_ATTEMPTS {
            self.request_missing_controls()?;
            self.event_queue.roundtrip(&mut self.state)?;
            self.state.prune_failed_controls();

            if self.state.has_ready_controls() {
                if attempt > 0 {
                    tracing::info!(
                        attempt,
                        outputs = %self.state.ready_output_label_list(),
                        "night light backend: gamma control recovered after retry"
                    );
                }
                return Ok(());
            }

            if attempt < GAMMA_CONTROL_RETRY_ATTEMPTS {
                tracing::debug!(
                    attempt,
                    outputs = %self.state.output_label_list(),
                    "night light backend: gamma control not ready yet, retrying"
                );
                std::thread::sleep(GAMMA_CONTROL_RETRY_DELAY);
            }
        }

        anyhow::bail!(
            "no outputs accepted gamma control for {}; another gamma controller may already be active or the compositor may still be releasing control",
            self.state.output_label_list()
        );
    }

    fn request_missing_controls(&mut self) -> anyhow::Result<()> {
        let qh = self.event_queue.handle();
        let manager = self
            .state
            .manager
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing gamma control manager"))?
            .clone();

        for output in &self.state.outputs {
            if self.state.controls.contains_key(&output.name) {
                continue;
            }
            tracing::debug!(
                output = %output.label(),
                "night light backend: requesting gamma control"
            );
            let control = manager.get_gamma_control(&output.output, &qh, output.name);
            self.state.controls.insert(
                output.name,
                GammaControlEntry {
                    control,
                    gamma_size: None,
                },
            );
        }

        Ok(())
    }
}

#[derive(Default)]
struct GammaState {
    manager: Option<zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1>,
    outputs: Vec<OutputEntry>,
    controls: HashMap<u32, GammaControlEntry>,
    failed_controls: HashSet<u32>,
}

impl GammaState {
    fn output_names(&self) -> Vec<u32> {
        self.outputs.iter().map(|output| output.name).collect()
    }

    fn output_label(&self, output_name: u32) -> String {
        self.outputs
            .iter()
            .find(|output| output.name == output_name)
            .map(OutputEntry::label)
            .unwrap_or_else(|| format!("output-{}", output_name))
    }

    fn output_label_list(&self) -> String {
        let labels = self
            .outputs
            .iter()
            .map(OutputEntry::label)
            .collect::<Vec<_>>();
        if labels.is_empty() {
            "no outputs".to_owned()
        } else {
            labels.join(", ")
        }
    }

    fn ready_output_label_list(&self) -> String {
        let labels = self
            .controls
            .iter()
            .filter(|(_, entry)| entry.gamma_size.is_some())
            .map(|(output_name, _)| self.output_label(*output_name))
            .collect::<Vec<_>>();
        if labels.is_empty() {
            self.output_label_list()
        } else {
            labels.join(", ")
        }
    }

    fn has_ready_controls(&self) -> bool {
        self.controls
            .values()
            .any(|entry| entry.gamma_size.is_some())
    }

    fn prune_failed_controls(&mut self) {
        let failed = self.failed_controls.drain().collect::<Vec<_>>();
        for output_name in failed {
            let output_label = self.output_label(output_name);
            if let Some(entry) = self.controls.remove(&output_name) {
                entry.control.destroy();
            }
            tracing::debug!(
                output = %output_label,
                "night light backend: gamma control failed for output"
            );
        }
    }
}

struct OutputEntry {
    name: u32,
    output: wl_output::WlOutput,
    logical_name: Option<String>,
    description: Option<String>,
}

impl OutputEntry {
    fn label(&self) -> String {
        self.logical_name
            .clone()
            .or_else(|| self.description.clone())
            .unwrap_or_else(|| format!("output-{}", self.name))
    }
}

struct GammaControlEntry {
    control: zwlr_gamma_control_v1::ZwlrGammaControlV1,
    gamma_size: Option<u32>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for GammaState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_output" => {
                    let output =
                        registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, name);
                    state.outputs.push(OutputEntry {
                        name,
                        output,
                        logical_name: None,
                        description: None,
                    });
                }
                "zwlr_gamma_control_manager_v1" => {
                    if state.manager.is_none() {
                        let manager = registry
                            .bind::<zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1, _, _>(
                            name,
                            version.min(1),
                            qh,
                            (),
                        );
                        state.manager = Some(manager);
                    }
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name } => {
                state.outputs.retain(|output| output.name != name);
                if let Some(entry) = state.controls.remove(&name) {
                    entry.control.destroy();
                }
                state.failed_controls.remove(&name);
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for GammaState {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        output_name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(output) = state
            .outputs
            .iter_mut()
            .find(|output| output.name == *output_name)
        else {
            return;
        };

        match event {
            wl_output::Event::Name { name } => {
                output.logical_name = Some(name);
            }
            wl_output::Event::Description { description } => {
                output.description = Some(description);
            }
            _ => {}
        }
    }
}

impl Dispatch<zwlr_gamma_control_v1::ZwlrGammaControlV1, u32> for GammaState {
    fn event(
        state: &mut Self,
        _: &zwlr_gamma_control_v1::ZwlrGammaControlV1,
        event: zwlr_gamma_control_v1::Event,
        output_name: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_gamma_control_v1::Event::GammaSize { size } => {
                if let Some(entry) = state.controls.get_mut(output_name) {
                    entry.gamma_size = Some(size);
                }
            }
            zwlr_gamma_control_v1::Event::Failed => {
                state.failed_controls.insert(*output_name);
            }
            _ => {}
        }
    }
}

delegate_noop!(GammaState: ignore zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1);

fn write_gamma_ramp(
    gamma_size: u32,
    red_scale: f32,
    green_scale: f32,
    blue_scale: f32,
) -> anyhow::Result<File> {
    let path = temp_gamma_path();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    let _ = std::fs::remove_file(&path);

    for channel_scale in [red_scale, green_scale, blue_scale] {
        for index in 0..gamma_size {
            let progress = if gamma_size <= 1 {
                1.0
            } else {
                index as f32 / (gamma_size - 1) as f32
            };
            let value = ((progress * channel_scale).clamp(0.0, 1.0) * u16::MAX as f32).round();
            file.write_all(&(value as u16).to_ne_bytes())?;
        }
    }

    file.flush()?;
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}

fn temp_gamma_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "glimpse-night-light-{}-{}.gamma",
        std::process::id(),
        Uuid::new_v4()
    ))
}

fn temperature_rgb_scales(temperature_kelvin: u32) -> (f32, f32, f32) {
    let kelvin = (temperature_kelvin as f32).clamp(1000.0, 40_000.0) / 100.0;

    let red = if kelvin <= 66.0 {
        255.0
    } else {
        329.69873 * (kelvin - 60.0).powf(-0.133_204_76)
    };

    let green = if kelvin <= 66.0 {
        99.470_8 * kelvin.ln() - 161.119_57
    } else {
        288.122_16 * (kelvin - 60.0).powf(-0.075_514_846)
    };

    let blue = if kelvin >= 66.0 {
        255.0
    } else if kelvin <= 19.0 {
        0.0
    } else {
        138.517_73 * (kelvin - 10.0).ln() - 305.044_8
    };

    (
        (red / 255.0).clamp(0.0, 1.0),
        (green / 255.0).clamp(0.0, 1.0),
        (blue / 255.0).clamp(0.0, 1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::{create_backend, temperature_rgb_scales, write_gamma_ramp};
    use crate::compositor::CompositorKind;
    use std::io::Read;

    #[test]
    fn supported_compositors_map_to_gamma_control_backend() {
        for compositor in [CompositorKind::Niri, CompositorKind::Hyprland] {
            let backend = create_backend(compositor);
            assert_eq!(backend.compositor(), compositor);
            assert!(backend.compositor_capabilities().night_light);
            assert!(
                backend
                    .compositor_capabilities()
                    .night_light_per_output_control
            );
        }
    }

    #[test]
    fn unknown_compositor_is_unsupported() {
        let backend = create_backend(CompositorKind::Unknown);
        assert!(!backend.compositor_capabilities().night_light);
        assert!(
            !backend
                .compositor_capabilities()
                .night_light_per_output_control
        );
    }

    #[test]
    fn temperature_scales_warm_blue_channel_down() {
        let neutral = temperature_rgb_scales(6500);
        let warm = temperature_rgb_scales(3500);
        assert!(warm.2 < neutral.2);
        assert!(warm.0 >= warm.2);
    }

    #[test]
    fn gamma_ramp_has_expected_byte_length() {
        let mut file = write_gamma_ramp(4, 1.0, 0.5, 0.25).expect("gamma ramp");
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).expect("ramp bytes");
        assert_eq!(bytes.len(), 4 * 3 * 2);
    }
}
