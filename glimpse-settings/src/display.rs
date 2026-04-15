use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{LazyLock, Mutex},
};

use crate::niri_managed;
use gtk4::{self as gtk, gio, prelude::*};
use serde::Deserialize;

pub use glimpse::compositor::CompositorKind;

pub fn managed_displays_path(kind: CompositorKind) -> &'static str {
    match kind {
        CompositorKind::Niri => "~/.config/niri/glimpse.d/displays.kdl",
        CompositorKind::Hyprland => "~/.config/hypr/glimpse.d/displays.conf",
        CompositorKind::Unknown => "~/.config/compositor/glimpse.d/displays.conf",
    }
}

pub fn managed_include_path(kind: CompositorKind) -> &'static str {
    match kind {
        CompositorKind::Niri => "~/.config/niri/glimpse.d/index.kdl",
        CompositorKind::Hyprland => "~/.config/hypr/glimpse.d/",
        CompositorKind::Unknown => "~/.config/compositor/glimpse.d/",
    }
}

pub fn managed_displays_runtime_path(kind: CompositorKind) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut path = PathBuf::from(home);
    match kind {
        CompositorKind::Niri => {
            path.push(".config/niri/glimpse.d/displays.kdl");
            Some(path)
        }
        CompositorKind::Hyprland => {
            path.push(".config/hypr/glimpse.d/displays.conf");
            Some(path)
        }
        CompositorKind::Unknown => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayOrientation {
    Landscape,
    PortraitRight,
    UpsideDown,
    PortraitLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayPlacement {
    Left,
    Right,
    Top,
    Bottom,
}

impl DisplayOrientation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Landscape => "Landscape",
            Self::PortraitRight => "Portrait Right",
            Self::UpsideDown => "Landscape (Flipped)",
            Self::PortraitLeft => "Portrait Left",
        }
    }

    pub fn all() -> &'static [DisplayOrientation] {
        &[
            DisplayOrientation::Landscape,
            DisplayOrientation::PortraitRight,
            DisplayOrientation::UpsideDown,
            DisplayOrientation::PortraitLeft,
        ]
    }

    pub fn from_niri_transform(value: &str) -> Self {
        match value {
            "Flipped180" | "180" => Self::UpsideDown,
            "90" | "Flipped90" => Self::PortraitRight,
            "270" | "Flipped270" => Self::PortraitLeft,
            _ => Self::Landscape,
        }
    }

    pub fn from_hypr_transform(value: i32) -> Self {
        match value {
            1 | 5 => Self::PortraitRight,
            2 | 6 => Self::UpsideDown,
            3 | 7 => Self::PortraitLeft,
            _ => Self::Landscape,
        }
    }

    fn is_portrait(self) -> bool {
        matches!(self, Self::PortraitRight | Self::PortraitLeft)
    }

    pub fn niri_transform(self) -> &'static str {
        match self {
            Self::Landscape => "normal",
            Self::PortraitRight => "90",
            Self::UpsideDown => "180",
            Self::PortraitLeft => "270",
        }
    }

    pub fn hypr_transform(self) -> i32 {
        match self {
            Self::Landscape => 0,
            Self::PortraitRight => 1,
            Self::UpsideDown => 2,
            Self::PortraitLeft => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_millihz: u32,
    pub preferred: bool,
}

impl DisplayMode {
    pub fn resolution_label(&self) -> String {
        format!("{} × {}", self.width, self.height)
    }

    pub fn refresh_label(&self) -> String {
        if self.refresh_millihz % 1000 == 0 {
            format!("{} Hz", self.refresh_millihz / 1000)
        } else {
            format!("{:.3} Hz", self.refresh_millihz as f64 / 1000.0)
        }
    }

    pub fn niri_mode_string(&self) -> String {
        format!(
            "{}x{}@{:.3}",
            self.width,
            self.height,
            self.refresh_millihz as f64 / 1000.0
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayOutput {
    pub id: String,
    pub title: String,
    pub connector: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub physical_size_mm: Option<(u32, u32)>,
    pub edid: Option<EdidInfo>,
    pub enabled: bool,
    pub primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
    pub orientation: DisplayOrientation,
    pub current_mode: DisplayMode,
    pub available_modes: Vec<DisplayMode>,
    pub vrr_enabled: Option<bool>,
    pub hdr_enabled: Option<bool>,
    pub ten_bit_enabled: Option<bool>,
    pub mirror_source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EdidInfo {
    pub display_class: Option<String>,
    pub manufacture_date: Option<String>,
    pub bits_per_primary_channel: Option<String>,
    pub panel_technology: Option<String>,
    pub native_color_depth: Option<String>,
    pub supported_input_formats: Option<String>,
    pub supported_transport_depths: Option<String>,
    pub color_capabilities: Option<String>,
    pub hdr_capabilities: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct HyprManagedState {
    enabled: Option<bool>,
    hdr_enabled: Option<bool>,
    ten_bit_enabled: Option<bool>,
    mirror_source: Option<String>,
}

static EDID_CACHE: LazyLock<Mutex<std::collections::HashMap<String, Option<EdidInfo>>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

impl DisplayOutput {
    fn preview_origin(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    fn transformed_mode_size(&self) -> (i32, i32) {
        let (mut width, mut height) = (
            self.current_mode.width.max(1) as i32,
            self.current_mode.height.max(1) as i32,
        );
        if self.orientation.is_portrait() {
            std::mem::swap(&mut width, &mut height);
        }
        (width, height)
    }

    fn logical_size(&self) -> (i32, i32) {
        let (width, height) = self.transformed_mode_size();
        let scale = self.scale.max(0.1);
        (
            ((width as f64) / scale).round().max(1.0) as i32,
            ((height as f64) / scale).round().max(1.0) as i32,
        )
    }

    fn preview_size(&self) -> (i32, i32) {
        self.logical_size()
    }

    fn sync_logical_geometry(&mut self) {
        let (width, height) = self.logical_size();
        self.width = width;
        self.height = height;
    }

    pub fn scale_label(&self) -> String {
        format!("{:.1}×", self.scale)
    }

    pub fn connector_label(&self) -> &str {
        &self.connector
    }

    pub fn make_label(&self) -> &str {
        self.make.as_deref().unwrap_or("Unavailable")
    }

    pub fn model_label(&self) -> &str {
        self.model.as_deref().unwrap_or("Unavailable")
    }

    pub fn serial_label(&self) -> &str {
        self.serial.as_deref().unwrap_or("Unavailable")
    }

    pub fn physical_size_label(&self) -> String {
        match self.physical_size_mm {
            Some((width, height)) => format!("{width} × {height} mm"),
            None => "Unavailable".into(),
        }
    }

    pub fn manufacture_date_label(&self) -> &str {
        self.edid
            .as_ref()
            .and_then(|edid| edid.manufacture_date.as_deref())
            .unwrap_or("Unavailable")
    }

    pub fn display_class_label(&self) -> &str {
        self.edid
            .as_ref()
            .and_then(|edid| edid.display_class.as_deref())
            .unwrap_or("Unavailable")
    }

    pub fn channel_depth_label(&self) -> String {
        self.edid
            .as_ref()
            .and_then(|edid| {
                edid.native_color_depth
                    .clone()
                    .or_else(|| edid.bits_per_primary_channel.clone())
            })
            .unwrap_or_else(|| "Unavailable".into())
    }

    pub fn panel_technology_label(&self) -> &str {
        self.edid
            .as_ref()
            .and_then(|edid| edid.panel_technology.as_deref())
            .unwrap_or("Unavailable")
    }

    pub fn input_formats_label(&self) -> &str {
        self.edid
            .as_ref()
            .and_then(|edid| edid.supported_input_formats.as_deref())
            .unwrap_or("Unavailable")
    }

    pub fn transport_depths_label(&self) -> &str {
        self.edid
            .as_ref()
            .and_then(|edid| edid.supported_transport_depths.as_deref())
            .unwrap_or("Unavailable")
    }

    pub fn color_capabilities_label(&self) -> &str {
        self.edid
            .as_ref()
            .and_then(|edid| edid.color_capabilities.as_deref())
            .unwrap_or("Unavailable")
    }

    pub fn hdr_capabilities_label(&self) -> &str {
        self.edid
            .as_ref()
            .and_then(|edid| edid.hdr_capabilities.as_deref())
            .unwrap_or("Unavailable")
    }

    pub fn supports_hdr(&self) -> bool {
        self.hdr_enabled.is_some() || self.edid.as_ref().is_some_and(EdidInfo::supports_hdr)
    }

    pub fn supports_ten_bit(&self) -> bool {
        self.ten_bit_enabled.is_some() || self.edid.as_ref().is_some_and(EdidInfo::supports_ten_bit)
    }

    pub fn vrr_label(&self, compositor: CompositorKind) -> String {
        match self.vrr_enabled {
            Some(true) => "On".into(),
            Some(false) => "Off".into(),
            None => format!("Unavailable on {}", compositor.label()),
        }
    }

    pub fn hdr_label(&self, compositor: CompositorKind) -> String {
        match self.hdr_enabled {
            Some(true) => "On".into(),
            Some(false) => "Off".into(),
            None => format!("Unavailable on {}", compositor.label()),
        }
    }

    pub fn color_depth_label(&self, compositor: CompositorKind) -> String {
        match self.ten_bit_enabled {
            Some(true) => "10-bit".into(),
            Some(false) => "8-bit".into(),
            None => format!("Unavailable on {}", compositor.label()),
        }
    }

    #[cfg(test)]
    pub fn test(id: &str, x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            id: id.into(),
            title: id.into(),
            connector: id.into(),
            make: None,
            model: None,
            serial: None,
            physical_size_mm: None,
            edid: None,
            enabled: true,
            primary: false,
            x,
            y,
            width,
            height,
            scale: 1.0,
            orientation: DisplayOrientation::Landscape,
            current_mode: DisplayMode {
                width: width as u32,
                height: height as u32,
                refresh_millihz: 60_000,
                preferred: true,
            },
            available_modes: vec![DisplayMode {
                width: width as u32,
                height: height as u32,
                refresh_millihz: 60_000,
                preferred: true,
            }],
            vrr_enabled: Some(false),
            hdr_enabled: None,
            ten_bit_enabled: None,
            mirror_source: None,
        }
    }
}

impl EdidInfo {
    fn supports_hdr(&self) -> bool {
        self.hdr_capabilities.is_some()
    }

    fn supports_ten_bit(&self) -> bool {
        self.native_color_depth
            .as_deref()
            .or(self.bits_per_primary_channel.as_deref())
            .is_some_and(|value| value.contains("10") || value.contains("12"))
            || self
                .supported_transport_depths
                .as_deref()
                .is_some_and(|value| value.contains("10"))
    }
}

#[derive(Debug, Clone)]
pub struct DisplaySnapshot {
    pub compositor: CompositorKind,
    pub outputs: Vec<DisplayOutput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayDraft {
    pub compositor: CompositorKind,
    pub outputs: Vec<DisplayOutput>,
    pub selected_output_id: Option<String>,
    pub mirror: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalSnapshotUpdate {
    Unchanged,
    SyncedClean,
    BaselineUpdated,
    DraftReset,
}

impl DisplayDraft {
    pub fn from_snapshot(snapshot: DisplaySnapshot) -> Self {
        let selected_output_id = snapshot.primary_output().map(|output| output.id.clone());
        let mut draft = Self {
            compositor: snapshot.compositor,
            outputs: snapshot.outputs,
            selected_output_id,
            mirror: false,
        };
        draft.normalize_to_primary_origin();
        draft
    }

    pub fn selected_output(&self) -> Option<&DisplayOutput> {
        self.selected_output_id
            .as_ref()
            .and_then(|id| self.outputs.iter().find(|output| &output.id == id))
            .or_else(|| self.outputs.first())
    }

    pub fn select_output(&mut self, id: &str) {
        if self.outputs.iter().any(|output| output.id == id) {
            self.selected_output_id = Some(id.to_owned());
        }
    }

    pub fn set_primary_output(&mut self, id: &str) {
        if !self.outputs.iter().any(|output| output.id == id) {
            return;
        }

        for output in &mut self.outputs {
            output.primary = output.id == id;
        }
        self.selected_output_id = Some(id.to_owned());
        self.normalize_to_primary_origin();
    }

    pub fn place_selected_relative_to_primary(&mut self, placement: DisplayPlacement) {
        let Some(selected_id) = self.selected_output_id.clone() else {
            return;
        };
        let Some(primary_index) = self.outputs.iter().position(|output| output.primary) else {
            return;
        };
        let Some(selected_index) = self
            .outputs
            .iter()
            .position(|output| output.id == selected_id)
        else {
            return;
        };
        if primary_index == selected_index {
            return;
        }

        let primary = self.outputs[primary_index].clone();
        let selected = self.outputs[selected_index].clone();
        let (primary_width, primary_height) = primary.logical_size();
        let (selected_width, selected_height) = selected.logical_size();
        let side_outputs = self
            .outputs
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != selected_index)
            .filter(|(_, output)| is_on_side_of_primary(output, &primary, placement))
            .map(|(_, output)| output.clone())
            .collect::<Vec<_>>();
        let (x, y) = match placement {
            DisplayPlacement::Left => (
                primary.x - selected_width,
                next_vertical_stack_slot(&side_outputs, primary.y, selected_height),
            ),
            DisplayPlacement::Right => (
                primary.x + primary_width,
                next_vertical_stack_slot(&side_outputs, primary.y, selected_height),
            ),
            DisplayPlacement::Top => (
                next_horizontal_stack_slot(&side_outputs, primary.x, selected_width),
                primary.y - selected_height,
            ),
            DisplayPlacement::Bottom => (
                next_horizontal_stack_slot(&side_outputs, primary.x, selected_width),
                primary.y + primary_height,
            ),
        };
        self.outputs[selected_index].x = x;
        self.outputs[selected_index].y = y;
        self.normalize_to_primary_origin();
    }

    pub fn move_output_by(&mut self, id: &str, dx: i32, dy: i32) {
        if let Some(output) = self.outputs.iter_mut().find(|output| output.id == id) {
            output.x += dx;
            output.y += dy;
        }
        self.normalize_to_primary_origin();
    }

    pub fn set_selected_enabled(&mut self, enabled: bool) {
        if let Some(output) = self.selected_output_mut() {
            output.enabled = enabled;
        }
    }

    pub fn set_selected_vrr_enabled(&mut self, enabled: bool) {
        if let Some(output) = self.selected_output_mut() {
            if output.vrr_enabled.is_some() {
                output.vrr_enabled = Some(enabled);
            }
        }
    }

    pub fn set_selected_hdr_enabled(&mut self, enabled: bool) {
        if let Some(output) = self.selected_output_mut() {
            if output.hdr_enabled.is_some() {
                output.hdr_enabled = Some(enabled);
            }
        }
    }

    pub fn set_selected_ten_bit_enabled(&mut self, enabled: bool) {
        if let Some(output) = self.selected_output_mut() {
            if output.ten_bit_enabled.is_some() {
                output.ten_bit_enabled = Some(enabled);
            }
        }
    }

    pub fn set_selected_mirror_source(&mut self, mirror_source: Option<&str>) {
        let Some(selected_id) = self.selected_output_id.clone() else {
            return;
        };
        let mirror_source = mirror_source
            .filter(|source| *source != selected_id.as_str())
            .map(str::to_owned);
        if let Some(output) = self.selected_output_mut() {
            output.mirror_source = mirror_source;
        }
    }

    pub fn set_selected_mode_index(&mut self, index: usize) {
        self.update_selected_geometry(|output| {
            if let Some(mode) = output.available_modes.get(index).cloned() {
                output.current_mode = mode;
                output.sync_logical_geometry();
            }
        });
    }

    pub fn set_selected_scale(&mut self, scale: f64) {
        self.update_selected_geometry(|output| {
            output.scale = scale;
            output.sync_logical_geometry();
        });
    }

    pub fn set_selected_orientation(&mut self, orientation: DisplayOrientation) {
        self.update_selected_geometry(|output| {
            output.orientation = orientation;
            output.sync_logical_geometry();
        });
    }

    pub fn is_dirty_against(&self, baseline: &DisplayDraft) -> bool {
        self.compositor != baseline.compositor
            || self.outputs != baseline.outputs
            || self.mirror != baseline.mirror
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.enabled_output_count() == 0 {
            return Err("at least one display must remain enabled".into());
        }
        Ok(())
    }

    fn selected_output_mut(&mut self) -> Option<&mut DisplayOutput> {
        let id = self.selected_output_id.clone();
        if let Some(id) = id {
            self.outputs.iter_mut().find(|output| output.id == id)
        } else {
            self.outputs.first_mut()
        }
    }

    fn normalize_to_primary_origin(&mut self) {
        let Some((origin_x, origin_y)) = self
            .outputs
            .iter()
            .find(|output| output.primary)
            .map(|output| (output.x, output.y))
        else {
            return;
        };

        if origin_x == 0 && origin_y == 0 {
            return;
        }

        for output in &mut self.outputs {
            output.x -= origin_x;
            output.y -= origin_y;
        }
    }

    fn enabled_output_count(&self) -> usize {
        self.outputs.iter().filter(|output| output.enabled).count()
    }

    fn update_selected_geometry<F>(&mut self, update: F)
    where
        F: FnOnce(&mut DisplayOutput),
    {
        let selected_id = self.selected_output_id.clone();
        let Some(index) = selected_id
            .as_ref()
            .and_then(|id| self.outputs.iter().position(|output| &output.id == id))
            .or_else(|| (!self.outputs.is_empty()).then_some(0))
        else {
            return;
        };

        let previous = self.outputs[index].clone();
        update(&mut self.outputs[index]);
        self.realign_outputs_after_geometry_change(index, &previous);
        self.normalize_to_primary_origin();
    }

    fn realign_outputs_after_geometry_change(
        &mut self,
        changed_index: usize,
        previous: &DisplayOutput,
    ) {
        let current = self.outputs[changed_index].clone();
        let old_left = previous.x;
        let old_top = previous.y;
        let (old_width, old_height) = previous.logical_size();
        let old_right = old_left + old_width;
        let old_bottom = old_top + old_height;

        let new_left = current.x;
        let new_top = current.y;
        let (new_width, new_height) = current.logical_size();
        let new_right = new_left + new_width;
        let new_bottom = new_top + new_height;

        let delta_left = new_left - old_left;
        let delta_right = new_right - old_right;
        let delta_top = new_top - old_top;
        let delta_bottom = new_bottom - old_bottom;

        for (index, output) in self.outputs.iter_mut().enumerate() {
            if index == changed_index {
                continue;
            }

            let other_left = output.x;
            let other_top = output.y;
            let (other_width, other_height) = output.logical_size();
            let other_right = other_left + other_width;
            let other_bottom = other_top + other_height;
            let overlaps_vertical = ranges_overlap(other_top, other_bottom, old_top, old_bottom);
            let overlaps_horizontal = ranges_overlap(other_left, other_right, old_left, old_right);

            let mut dx = 0;
            let mut dy = 0;

            if delta_right != 0 && other_left >= old_right && overlaps_vertical {
                dx += delta_right;
            }
            if delta_left != 0 && other_right <= old_left && overlaps_vertical {
                dx += delta_left;
            }
            if delta_bottom != 0 && other_top >= old_bottom && overlaps_horizontal {
                dy += delta_bottom;
            }
            if delta_top != 0 && other_bottom <= old_top && overlaps_horizontal {
                dy += delta_top;
            }

            output.x += dx;
            output.y += dy;
        }
    }
}

pub fn reconcile_external_snapshot(
    draft: &mut DisplayDraft,
    baseline: &mut DisplayDraft,
    snapshot: DisplaySnapshot,
) -> ExternalSnapshotUpdate {
    let selected_output_id = draft
        .selected_output_id
        .clone()
        .or_else(|| baseline.selected_output_id.clone());
    let mut fresh = DisplayDraft::from_snapshot(snapshot);
    if let Some(selected_id) = selected_output_id.as_deref() {
        fresh.select_output(selected_id);
    }

    if &fresh == baseline {
        return ExternalSnapshotUpdate::Unchanged;
    }

    let same_topology = same_output_ids(draft, &fresh);
    if !draft.is_dirty_against(baseline) {
        *baseline = fresh.clone();
        *draft = fresh;
        return ExternalSnapshotUpdate::SyncedClean;
    }

    if same_topology {
        *baseline = fresh;
        return ExternalSnapshotUpdate::BaselineUpdated;
    }

    *baseline = fresh.clone();
    *draft = fresh;
    ExternalSnapshotUpdate::DraftReset
}

fn same_output_ids(left: &DisplayDraft, right: &DisplayDraft) -> bool {
    left.outputs.len() == right.outputs.len()
        && left
            .outputs
            .iter()
            .map(|output| output.id.as_str())
            .eq(right.outputs.iter().map(|output| output.id.as_str()))
}

fn ranges_overlap(a_start: i32, a_end: i32, b_start: i32, b_end: i32) -> bool {
    a_start < b_end && b_start < a_end
}

fn is_on_side_of_primary(
    output: &DisplayOutput,
    primary: &DisplayOutput,
    placement: DisplayPlacement,
) -> bool {
    let (primary_width, primary_height) = primary.logical_size();
    let (output_width, output_height) = output.logical_size();
    match placement {
        DisplayPlacement::Left => output.x + output_width <= primary.x,
        DisplayPlacement::Right => output.x >= primary.x + primary_width,
        DisplayPlacement::Top => output.y + output_height <= primary.y,
        DisplayPlacement::Bottom => output.y >= primary.y + primary_height,
    }
}

fn next_vertical_stack_slot(outputs: &[DisplayOutput], start_y: i32, selected_height: i32) -> i32 {
    let mut ranges = outputs
        .iter()
        .map(|output| {
            let (_, height) = output.logical_size();
            (output.y, output.y + height)
        })
        .collect::<Vec<_>>();
    ranges.sort_by_key(|(top, _)| *top);

    let mut cursor = start_y;
    for (top, bottom) in ranges {
        if bottom <= cursor {
            continue;
        }
        if top >= cursor + selected_height {
            break;
        }
        cursor = bottom;
    }
    cursor
}

fn next_horizontal_stack_slot(outputs: &[DisplayOutput], start_x: i32, selected_width: i32) -> i32 {
    let mut ranges = outputs
        .iter()
        .map(|output| {
            let (width, _) = output.logical_size();
            (output.x, output.x + width)
        })
        .collect::<Vec<_>>();
    ranges.sort_by_key(|(left, _)| *left);

    let mut cursor = start_x;
    for (left, right) in ranges {
        if right <= cursor {
            continue;
        }
        if left >= cursor + selected_width {
            break;
        }
        cursor = right;
    }
    cursor
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistStatus {
    Applied {
        path: PathBuf,
        include_present: bool,
        reloaded: bool,
    },
    Unsupported,
}

pub fn apply_persisted_displays(draft: &DisplayDraft) -> Result<PersistStatus, String> {
    draft.validate()?;
    match draft.compositor {
        CompositorKind::Niri => apply_niri_persisted_displays(draft),
        CompositorKind::Hyprland => apply_hypr_persisted_displays(draft),
        CompositorKind::Unknown => Ok(PersistStatus::Unsupported),
    }
}

fn apply_niri_persisted_displays(draft: &DisplayDraft) -> Result<PersistStatus, String> {
    let mut draft = draft.clone();
    draft.normalize_to_primary_origin();
    let Some(path) = managed_displays_runtime_path(CompositorKind::Niri) else {
        return Err("HOME is not set".into());
    };
    let parent = path
        .parent()
        .ok_or("managed file has no parent directory")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::write(&path, serialize_niri_draft(&draft)).map_err(|error| error.to_string())?;
    let index_path = niri_managed::update_niri_index(&["displays.kdl"])?;

    let include_present = niri_managed::detect_niri_include(&index_path);
    finalize_niri_apply(
        path,
        include_present,
        niri_managed::validate_niri_config,
        niri_managed::reload_niri_config,
    )
}

fn apply_hypr_persisted_displays(draft: &DisplayDraft) -> Result<PersistStatus, String> {
    let mut draft = draft.clone();
    draft.normalize_to_primary_origin();
    let Some(path) = managed_displays_runtime_path(CompositorKind::Hyprland) else {
        return Err("HOME is not set".into());
    };
    let parent = path
        .parent()
        .ok_or("managed file has no parent directory")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::write(&path, serialize_hypr_draft(&draft)).map_err(|error| error.to_string())?;

    let include_present = detect_hypr_include(parent, &path);
    if include_present {
        reload_hypr_config()?;
    }

    Ok(PersistStatus::Applied {
        path,
        include_present,
        reloaded: include_present,
    })
}

fn detect_hypr_include(managed_dir: &Path, managed_path: &Path) -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    let config_path = PathBuf::from(home).join(".config/hypr/hyprland.conf");
    let Ok(config) = fs::read_to_string(config_path) else {
        return false;
    };
    let managed_dir = managed_dir.to_string_lossy();
    let managed_path = managed_path.to_string_lossy();
    config.contains(managed_dir.as_ref())
        || config.contains(managed_path.as_ref())
        || config.contains("~/.config/hypr/glimpse.d/")
        || config.contains("~/.config/hypr/glimpse.d/*")
        || config.contains(".config/hypr/glimpse.d/")
}

fn finalize_niri_apply<V, F>(
    path: PathBuf,
    include_present: bool,
    validate: V,
    reload: F,
) -> Result<PersistStatus, String>
where
    V: FnOnce(&Path) -> Result<(), String>,
    F: FnOnce() -> Result<(), String>,
{
    validate(&path)?;

    if include_present {
        reload()?;
    }

    Ok(PersistStatus::Applied {
        path,
        include_present,
        reloaded: include_present,
    })
}

fn reload_hypr_config() -> Result<(), String> {
    let output = Command::new("hyprctl")
        .args(["reload", "config-only"])
        .output()
        .map_err(|error| format!("failed to run hyprctl reload: {error}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        Err("hyprctl rejected the generated config reload".into())
    } else {
        Err(format!("hyprctl reload failed: {stderr}"))
    }
}

fn serialize_niri_draft(draft: &DisplayDraft) -> String {
    let mut output =
        String::from("// Generated by Glimpse Settings. Manual edits may be overwritten.\n");
    for display in &draft.outputs {
        output.push_str(&format!("output \"{}\" {{\n", display.id));
        if !display.enabled {
            output.push_str("    off\n");
            output.push_str("}\n\n");
            continue;
        }

        output.push_str(&format!(
            "    mode \"{}\"\n",
            display.current_mode.niri_mode_string()
        ));
        output.push_str(&format!("    position x={} y={}\n", display.x, display.y));
        output.push_str(&format!("    scale {:.2}\n", display.scale));
        output.push_str(&format!(
            "    transform \"{}\"\n",
            display.orientation.niri_transform()
        ));
        if display.vrr_enabled == Some(true) {
            output.push_str("    variable-refresh-rate\n");
        }
        if display.primary {
            output.push_str("    focus-at-startup\n");
        }
        output.push_str("}\n\n");
    }
    output
}

fn serialize_hypr_draft(draft: &DisplayDraft) -> String {
    let mut output =
        String::from("# Generated by Glimpse Settings. Manual edits may be overwritten.\n");
    for display in &draft.outputs {
        if !display.enabled {
            output.push_str(&format!("monitor = {}, disable\n", display.id));
            continue;
        }

        let mut line = format!(
            "monitor = {}, {}, {}x{}, {:.2}",
            display.id,
            display.current_mode.niri_mode_string(),
            display.x,
            display.y,
            display.scale
        );

        if display.orientation != DisplayOrientation::Landscape {
            line.push_str(&format!(
                ", transform, {}",
                display.orientation.hypr_transform()
            ));
        }
        if let Some(vrr_enabled) = display.vrr_enabled {
            line.push_str(&format!(", vrr, {}", if vrr_enabled { 1 } else { 0 }));
        }
        if let Some(mirror_source) = display.mirror_source.as_deref() {
            line.push_str(&format!(", mirror, {mirror_source}"));
        }
        if display.ten_bit_enabled == Some(true) {
            line.push_str(", bitdepth, 10");
        }
        if display.hdr_enabled == Some(true) {
            line.push_str(", cm, hdr");
        }

        output.push_str(&line);
        output.push('\n');
    }

    output
}

impl DisplaySnapshot {
    pub fn current() -> Self {
        let compositor = CompositorKind::detect();
        if compositor == CompositorKind::Niri {
            if let Some(snapshot) = snapshot_from_niri_msg() {
                return snapshot;
            }
        } else if compositor == CompositorKind::Hyprland {
            if let Some(snapshot) = snapshot_from_hyprctl() {
                return snapshot;
            }
        }

        if let Some(display) = gtk::gdk::Display::default() {
            let snapshot = Self::from_gdk(&display, compositor);
            if !snapshot.outputs.is_empty() {
                return snapshot;
            }
        }

        Self::empty(compositor)
    }

    pub fn primary_output(&self) -> Option<&DisplayOutput> {
        self.outputs
            .iter()
            .find(|output| output.primary)
            .or_else(|| self.outputs.first())
    }

    fn from_gdk(display: &gtk::gdk::Display, compositor: CompositorKind) -> Self {
        let monitors: gio::ListModel = display.monitors();
        let mut outputs = Vec::new();

        for index in 0..monitors.n_items() {
            let Some(item) = monitors.item(index) else {
                continue;
            };
            let Ok(monitor) = item.downcast::<gtk::gdk::Monitor>() else {
                continue;
            };
            let geometry = monitor.geometry();
            let width = geometry.width();
            let height = geometry.height();
            if width <= 0 || height <= 0 {
                continue;
            }

            let connector = monitor
                .connector()
                .map(|value| value.to_string())
                .filter(|value| !value.is_empty());
            let title = connector.clone().unwrap_or_else(|| {
                let manufacturer = monitor.manufacturer().map(|value| value.to_string());
                let model = monitor.model().map(|value| value.to_string());
                match (manufacturer, model) {
                    (Some(make), Some(model)) => format!("{make} {model}"),
                    (Some(make), None) => make,
                    (None, Some(model)) => model,
                    (None, None) => format!("Display {}", index + 1),
                }
            });

            let refresh_raw = monitor.refresh_rate().max(60_000) as u32;

            outputs.push(DisplayOutput {
                id: connector
                    .clone()
                    .unwrap_or_else(|| format!("display-{}", index + 1)),
                title,
                connector: connector
                    .clone()
                    .unwrap_or_else(|| format!("display-{}", index + 1)),
                make: monitor.manufacturer().map(|value| value.to_string()),
                model: monitor.model().map(|value| value.to_string()),
                serial: None,
                physical_size_mm: match (monitor.width_mm(), monitor.height_mm()) {
                    (width, height) if width > 0 && height > 0 => {
                        Some((width as u32, height as u32))
                    }
                    _ => None,
                },
                edid: connector.as_deref().and_then(read_edid_info),
                enabled: true,
                primary: index == 0,
                x: geometry.x(),
                y: geometry.y(),
                width,
                height,
                scale: monitor.scale_factor() as f64,
                orientation: DisplayOrientation::Landscape,
                current_mode: DisplayMode {
                    width: width as u32,
                    height: height as u32,
                    refresh_millihz: refresh_raw,
                    preferred: true,
                },
                available_modes: vec![DisplayMode {
                    width: width as u32,
                    height: height as u32,
                    refresh_millihz: refresh_raw,
                    preferred: true,
                }],
                vrr_enabled: None,
                hdr_enabled: None,
                ten_bit_enabled: None,
                mirror_source: None,
            });
        }

        Self {
            compositor,
            outputs,
        }
    }

    fn empty(compositor: CompositorKind) -> Self {
        Self {
            compositor,
            outputs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutBox {
    pub id: String,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub primary: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayoutScene {
    pub boxes: Vec<LayoutBox>,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreviewRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl PreviewRect {
    fn right(self) -> i32 {
        self.x + self.width
    }

    fn bottom(self) -> i32 {
        self.y + self.height
    }
}

#[derive(Debug, Deserialize)]
struct NiriOutput {
    name: String,
    make: Option<String>,
    model: Option<String>,
    serial: Option<String>,
    physical_size: Option<[u32; 2]>,
    modes: Vec<NiriMode>,
    current_mode: Option<usize>,
    vrr_supported: bool,
    vrr_enabled: bool,
    logical: Option<NiriLogical>,
}

#[derive(Debug, Deserialize)]
struct NiriMode {
    width: u32,
    height: u32,
    refresh_rate: u32,
    is_preferred: bool,
}

#[derive(Debug, Deserialize)]
struct NiriLogical {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale: f64,
    transform: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HyprMonitor {
    name: String,
    description: Option<String>,
    make: Option<String>,
    model: Option<String>,
    serial: Option<String>,
    width: i32,
    height: i32,
    refresh_rate: f64,
    x: i32,
    y: i32,
    scale: f64,
    transform: i32,
    focused: bool,
    disabled: bool,
    vrr: Option<bool>,
    current_format: Option<String>,
    mirror_of: Option<String>,
    available_modes: Option<Vec<String>>,
}

fn snapshot_from_niri_msg() -> Option<DisplaySnapshot> {
    let output = Command::new("niri")
        .args(["msg", "--json", "outputs"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    snapshot_from_niri_json(&stdout).ok()
}

fn snapshot_from_hyprctl() -> Option<DisplaySnapshot> {
    let output = Command::new("hyprctl")
        .args(["-j", "monitors", "all"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let managed = read_hypr_managed_state();
    snapshot_from_hypr_json(&stdout, &managed).ok()
}

fn snapshot_from_niri_json(json: &str) -> Result<DisplaySnapshot, serde_json::Error> {
    let parsed: serde_json::Map<String, serde_json::Value> = serde_json::from_str(json)?;
    let mut active_bounds_right = 0;
    let mut outputs = Vec::new();
    let mut disabled = Vec::new();

    for value in parsed.into_values() {
        let output: NiriOutput = serde_json::from_value(value)?;
        let title = format_output_title(
            output.make.as_deref(),
            output.model.as_deref(),
            &output.name,
        );
        let current_mode = pick_niri_mode(&output);
        let available_modes = output
            .modes
            .iter()
            .map(|mode| DisplayMode {
                width: mode.width,
                height: mode.height,
                refresh_millihz: mode.refresh_rate,
                preferred: mode.is_preferred,
            })
            .collect::<Vec<_>>();
        let enabled = output.logical.is_some();

        if let Some(logical) = output.logical {
            active_bounds_right = active_bounds_right.max(logical.x + logical.width);
            outputs.push(DisplayOutput {
                id: output.name.clone(),
                title,
                connector: output.name.clone(),
                make: output.make.clone(),
                model: output.model.clone(),
                serial: output.serial.clone(),
                physical_size_mm: output.physical_size.map(|size| (size[0], size[1])),
                edid: read_edid_info(&output.name),
                enabled,
                primary: outputs.is_empty(),
                x: logical.x,
                y: logical.y,
                width: logical.width,
                height: logical.height,
                scale: logical.scale,
                orientation: DisplayOrientation::from_niri_transform(&logical.transform),
                current_mode,
                available_modes,
                vrr_enabled: output.vrr_supported.then_some(output.vrr_enabled),
                hdr_enabled: None,
                ten_bit_enabled: None,
                mirror_source: None,
            });
        } else {
            disabled.push((
                output.name,
                title,
                output.make,
                output.model,
                output.serial,
                output.physical_size,
                current_mode,
                available_modes,
                output.vrr_supported.then_some(output.vrr_enabled),
            ));
        }
    }

    let mut cursor_x = active_bounds_right;
    let fallback_y = outputs.iter().map(|output| output.y).min().unwrap_or(0);
    if cursor_x != 0 {
        cursor_x += 96;
    }

    for (
        name,
        title,
        make,
        model,
        serial,
        physical_size,
        current_mode,
        available_modes,
        vrr_enabled,
    ) in disabled
    {
        let connector = name.clone();
        let edid = read_edid_info(&name);
        outputs.push(DisplayOutput {
            id: name,
            title,
            connector,
            make,
            model,
            serial,
            physical_size_mm: physical_size.map(|size| (size[0], size[1])),
            edid,
            enabled: false,
            primary: false,
            x: cursor_x,
            y: fallback_y,
            width: current_mode.width as i32,
            height: current_mode.height as i32,
            scale: 1.0,
            orientation: DisplayOrientation::Landscape,
            current_mode: current_mode.clone(),
            available_modes,
            vrr_enabled,
            hdr_enabled: None,
            ten_bit_enabled: None,
            mirror_source: None,
        });
        cursor_x += current_mode.width as i32 + 96;
    }

    Ok(DisplaySnapshot {
        compositor: CompositorKind::Niri,
        outputs,
    })
}

fn snapshot_from_hypr_json(
    json: &str,
    managed: &std::collections::HashMap<String, HyprManagedState>,
) -> Result<DisplaySnapshot, serde_json::Error> {
    let parsed: Vec<HyprMonitor> = serde_json::from_str(json)?;
    let mut outputs = parsed
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let connector = monitor.name.clone();
            let title = format_output_title(
                monitor.make.as_deref(),
                monitor.model.as_deref().or(monitor.description.as_deref()),
                &monitor.name,
            );
            let edid = read_edid_info(&connector);
            let managed_state = managed.get(&connector).cloned().unwrap_or_default();
            let available_modes = monitor
                .available_modes
                .as_deref()
                .map(parse_hypr_mode_list)
                .filter(|modes| !modes.is_empty())
                .unwrap_or_default();
            let current_mode = if monitor.width > 0 && monitor.height > 0 {
                DisplayMode {
                    width: monitor.width as u32,
                    height: monitor.height as u32,
                    refresh_millihz: (monitor.refresh_rate.max(1.0) * 1000.0).round() as u32,
                    preferred: true,
                }
            } else {
                available_modes.first().cloned().unwrap_or(DisplayMode {
                    width: 1920,
                    height: 1080,
                    refresh_millihz: 60_000,
                    preferred: true,
                })
            };
            let available_modes = if available_modes.is_empty() {
                vec![current_mode.clone()]
            } else {
                available_modes
            };
            let ten_bit_active = monitor
                .current_format
                .as_deref()
                .is_some_and(is_hypr_ten_bit_format);
            let hdr_supported = edid.as_ref().is_some_and(EdidInfo::supports_hdr)
                || managed_state.hdr_enabled.is_some();
            let ten_bit_supported = ten_bit_active
                || edid.as_ref().is_some_and(EdidInfo::supports_ten_bit)
                || managed_state.ten_bit_enabled.is_some();

            DisplayOutput {
                id: connector.clone(),
                title,
                connector,
                make: monitor.make.clone(),
                model: monitor.model.clone(),
                serial: monitor.serial.filter(|value| !value.is_empty()),
                physical_size_mm: None,
                edid,
                enabled: managed_state.enabled.unwrap_or(!monitor.disabled),
                primary: monitor.focused || index == 0,
                x: monitor.x,
                y: monitor.y,
                width: monitor.width.max(1),
                height: monitor.height.max(1),
                scale: monitor.scale.max(0.1),
                orientation: DisplayOrientation::from_hypr_transform(monitor.transform),
                current_mode,
                available_modes,
                vrr_enabled: monitor.vrr,
                hdr_enabled: hdr_supported.then_some(managed_state.hdr_enabled.unwrap_or(false)),
                ten_bit_enabled: ten_bit_supported
                    .then_some(managed_state.ten_bit_enabled.unwrap_or(ten_bit_active)),
                mirror_source: normalize_hypr_mirror_name(
                    managed_state
                        .mirror_source
                        .as_deref()
                        .or(monitor.mirror_of.as_deref()),
                ),
            }
        })
        .collect::<Vec<_>>();

    if !outputs.iter().any(|output| output.primary) && !outputs.is_empty() {
        outputs[0].primary = true;
    }

    Ok(DisplaySnapshot {
        compositor: CompositorKind::Hyprland,
        outputs,
    })
}

fn pick_niri_mode(output: &NiriOutput) -> DisplayMode {
    let mode = output
        .current_mode
        .and_then(|index| output.modes.get(index))
        .or_else(|| output.modes.iter().find(|mode| mode.is_preferred))
        .or_else(|| output.modes.first());

    match mode {
        Some(mode) => DisplayMode {
            width: mode.width,
            height: mode.height,
            refresh_millihz: mode.refresh_rate,
            preferred: mode.is_preferred,
        },
        None => DisplayMode {
            width: 1920,
            height: 1080,
            refresh_millihz: 60_000,
            preferred: true,
        },
    }
}

fn parse_hypr_mode_list(modes: &[String]) -> Vec<DisplayMode> {
    modes
        .iter()
        .filter_map(|mode| parse_hypr_mode(mode))
        .collect()
}

fn parse_hypr_mode(text: &str) -> Option<DisplayMode> {
    let (resolution, refresh) = text.split_once('@')?;
    let (width, height) = resolution.split_once('x')?;
    let refresh = refresh.trim_end_matches("Hz");
    Some(DisplayMode {
        width: width.parse().ok()?,
        height: height.parse().ok()?,
        refresh_millihz: (refresh.parse::<f64>().ok()? * 1000.0).round() as u32,
        preferred: false,
    })
}

fn is_hypr_ten_bit_format(format: &str) -> bool {
    format.contains("2101010") || format.contains("30")
}

fn normalize_hypr_mirror_name(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "none")
        .map(str::to_owned)
}

fn format_output_title(make: Option<&str>, model: Option<&str>, fallback: &str) -> String {
    match (
        make.map(str::trim).filter(|value| !value.is_empty()),
        model.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(make), Some(model)) => format!("{make} {model}"),
        (Some(make), None) => make.to_string(),
        (None, Some(model)) => model.to_string(),
        (None, None) => fallback.to_string(),
    }
}

fn read_edid_info(connector: &str) -> Option<EdidInfo> {
    if let Ok(cache) = EDID_CACHE.lock() {
        if let Some(cached) = cache.get(connector) {
            return cached.clone();
        }
    }

    let info = find_edid_path(connector)
        .and_then(|path| decode_edid_info(&path).ok())
        .filter(|info| info != &EdidInfo::default());

    if let Ok(mut cache) = EDID_CACHE.lock() {
        cache.insert(connector.to_owned(), info.clone());
    }

    info
}

fn find_edid_path(connector: &str) -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/drm").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.ends_with(&format!("-{connector}")) {
            continue;
        }
        let path = entry.path().join("edid");
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn decode_edid_info(path: &Path) -> Result<EdidInfo, String> {
    let output = Command::new("edid-decode")
        .arg(path)
        .output()
        .map_err(|error| format!("failed to run edid-decode: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if stderr.is_empty() {
            "edid-decode failed".into()
        } else {
            stderr
        });
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("edid-decode output was not valid UTF-8: {error}"))?;
    Ok(parse_edid_decode(&stdout))
}

fn parse_edid_decode(text: &str) -> EdidInfo {
    let mut info = EdidInfo::default();
    let mut transport_depths = Vec::new();
    let mut color_capabilities = Vec::new();
    let mut hdr_capabilities = Vec::new();
    let mut in_colorimetry = false;
    let mut in_hdr_block = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            in_colorimetry = false;
            in_hdr_block = false;
            continue;
        }

        if trimmed.ends_with(':') {
            if trimmed == "Colorimetry Data Block:" {
                in_colorimetry = true;
                in_hdr_block = false;
            } else if trimmed == "HDR Static Metadata Data Block:" {
                in_colorimetry = false;
                in_hdr_block = true;
            } else if !in_hdr_block {
                in_colorimetry = false;
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("Made in: ") {
            info.manufacture_date = Some(rest.to_owned());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Display Product Type: ") {
            info.display_class = Some(rest.to_owned());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Display Product Primary Use Case: ") {
            if info.display_class.is_none() {
                info.display_class = Some(rest.to_owned());
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Bits per primary color channel: ") {
            info.bits_per_primary_channel = Some(format!("{rest} bpc"));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Display Device Technology: ") {
            info.panel_technology = Some(rest.to_owned());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Native Color Depth: ") {
            info.native_color_depth = Some(rest.to_owned());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Supported color formats: ") {
            info.supported_input_formats = Some(rest.to_owned());
            continue;
        }
        if trimmed.starts_with("Supported bpc for ") {
            transport_depths.push(trimmed.to_owned());
            continue;
        }
        if let Some((_, rest)) = trimmed.split_once(": ")
            && trimmed.starts_with("Supported color space and EOTF standard combination")
        {
            color_capabilities.push(rest.to_owned());
            continue;
        }
        if in_colorimetry {
            color_capabilities.push(trimmed.to_owned());
            continue;
        }
        if in_hdr_block
            && (trimmed == "Traditional gamma - SDR luminance range"
                || trimmed == "SMPTE ST2084"
                || trimmed == "Hybrid Log-Gamma")
        {
            hdr_capabilities.push(trimmed.to_owned());
        }
    }

    if !transport_depths.is_empty() {
        info.supported_transport_depths = Some(transport_depths.join("; "));
    }
    if !color_capabilities.is_empty() {
        info.color_capabilities = Some(color_capabilities.join("; "));
    }
    if !hdr_capabilities.is_empty() {
        let mut combined = vec!["HDR Static Metadata".to_owned()];
        combined.extend(hdr_capabilities);
        info.hdr_capabilities = Some(combined.join("; "));
    }

    info
}

fn read_hypr_managed_state() -> std::collections::HashMap<String, HyprManagedState> {
    let Some(path) = managed_displays_runtime_path(CompositorKind::Hyprland) else {
        return std::collections::HashMap::new();
    };
    let Ok(text) = fs::read_to_string(path) else {
        return std::collections::HashMap::new();
    };
    parse_hypr_managed_state(&text)
}

fn parse_hypr_managed_state(text: &str) -> std::collections::HashMap<String, HyprManagedState> {
    let mut state = std::collections::HashMap::new();

    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some(rest) = line.strip_prefix("monitor =") else {
            continue;
        };
        let parts = rest
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let Some(name) = parts.first() else {
            continue;
        };

        let mut entry = HyprManagedState::default();
        if parts.get(1).is_some_and(|part| *part == "disable") {
            entry.enabled = Some(false);
            state.insert((*name).to_owned(), entry);
            continue;
        }

        entry.enabled = Some(true);
        let mut index = 4;
        while index < parts.len() {
            match parts[index] {
                "mirror" => {
                    if let Some(target) = parts.get(index + 1) {
                        entry.mirror_source = Some((*target).to_owned());
                    }
                    index += 2;
                }
                "bitdepth" => {
                    entry.ten_bit_enabled = Some(parts.get(index + 1) == Some(&"10"));
                    index += 2;
                }
                "cm" => {
                    entry.hdr_enabled = Some(matches!(
                        parts.get(index + 1),
                        Some(&"hdr") | Some(&"hdredid")
                    ));
                    index += 2;
                }
                _ => {
                    index += 1;
                }
            }
        }

        state.insert((*name).to_owned(), entry);
    }

    state
}

pub fn layout_boxes(
    outputs: &[DisplayOutput],
    max_width: i32,
    max_height: i32,
    padding: i32,
) -> Vec<LayoutBox> {
    layout_scene(outputs, max_width, max_height, padding).boxes
}

pub fn layout_scene(
    outputs: &[DisplayOutput],
    max_width: i32,
    max_height: i32,
    padding: i32,
) -> LayoutScene {
    if outputs.is_empty() {
        return LayoutScene {
            boxes: Vec::new(),
            scale: 1.0,
        };
    }

    let preview_rects = outputs
        .iter()
        .map(|output| {
            let (x, y) = output.preview_origin();
            let (width, height) = output.preview_size();
            PreviewRect {
                x,
                y,
                width,
                height,
            }
        })
        .collect::<Vec<_>>();

    let min_x = preview_rects.iter().map(|rect| rect.x).min().unwrap_or(0);
    let min_y = preview_rects.iter().map(|rect| rect.y).min().unwrap_or(0);
    let max_x = preview_rects
        .iter()
        .map(|rect| rect.right())
        .max()
        .unwrap_or(1);
    let max_y = preview_rects
        .iter()
        .map(|rect| rect.bottom())
        .max()
        .unwrap_or(1);

    let bounds_width = (max_x - min_x).max(1) as f64;
    let bounds_height = (max_y - min_y).max(1) as f64;
    let usable_width = (max_width - padding * 2).max(1) as f64;
    let usable_height = (max_height - padding * 2).max(1) as f64;
    let scale = f64::min(usable_width / bounds_width, usable_height / bounds_height);
    let content_width = (bounds_width * scale).round() as i32;
    let content_height = (bounds_height * scale).round() as i32;
    let offset_x = padding + ((usable_width as i32 - content_width).max(0) / 2);
    let offset_y = padding + ((usable_height as i32 - content_height).max(0) / 2);

    let boxes = outputs
        .iter()
        .zip(preview_rects)
        .map(|(output, rect)| {
            let left = offset_x + (((rect.x - min_x) as f64) * scale).round() as i32;
            let top = offset_y + (((rect.y - min_y) as f64) * scale).round() as i32;
            let right = offset_x + (((rect.right() - min_x) as f64) * scale).round() as i32;
            let bottom = offset_y + (((rect.bottom() - min_y) as f64) * scale).round() as i32;

            LayoutBox {
                id: output.id.clone(),
                title: output.title.clone(),
                x: left,
                y: top,
                width: (right - left).max(1),
                height: (bottom - top).max(1),
                primary: output.primary,
                enabled: output.enabled,
            }
        })
        .collect();

    LayoutScene { boxes, scale }
}

impl DisplayDraft {
    pub fn preview_origin_for_output(&self, id: &str) -> Option<(i32, i32)> {
        self.outputs
            .iter()
            .find(|output| output.id == id)
            .map(DisplayOutput::preview_origin)
    }

    pub fn preview_drag_position(
        &self,
        id: &str,
        dx_preview: f64,
        dy_preview: f64,
        snap_threshold_preview: f64,
    ) -> Option<(i32, i32)> {
        let index = self.outputs.iter().position(|output| output.id == id)?;
        let output = self.outputs[index].clone();
        let (origin_x, origin_y) = output.preview_origin();
        let (width, height) = output.preview_size();
        let proposed = PreviewRect {
            x: origin_x + dx_preview.round() as i32,
            y: origin_y + dy_preview.round() as i32,
            width,
            height,
        };
        let others = self
            .outputs
            .iter()
            .enumerate()
            .filter(|(other_index, _)| *other_index != index)
            .map(|(_, other)| {
                let (x, y) = other.preview_origin();
                let (width, height) = other.preview_size();
                PreviewRect {
                    x,
                    y,
                    width,
                    height,
                }
            })
            .collect::<Vec<_>>();

        let snapped = snap_preview_rect(
            proposed,
            &others,
            snap_threshold_preview.round().max(0.0) as i32,
        );
        Some((snapped.x, snapped.y))
    }

    pub fn move_output_by_preview_delta(
        &mut self,
        id: &str,
        dx_preview: f64,
        dy_preview: f64,
        snap_threshold_preview: f64,
    ) {
        let Some(index) = self.outputs.iter().position(|output| output.id == id) else {
            return;
        };

        let output = self.outputs[index].clone();
        let Some((snapped_x, snapped_y)) =
            self.preview_drag_position(id, dx_preview, dy_preview, snap_threshold_preview)
        else {
            return;
        };
        let scale = output.scale.max(0.1);
        self.outputs[index].x = (snapped_x as f64 / scale).round() as i32;
        self.outputs[index].y = (snapped_y as f64 / scale).round() as i32;
        self.normalize_to_primary_origin();
    }
}

fn snap_preview_rect(rect: PreviewRect, others: &[PreviewRect], threshold: i32) -> PreviewRect {
    if others.is_empty() {
        return rect;
    }

    let mut best = None::<(i32, PreviewRect)>;
    for other in others {
        let horizontal_candidates = [
            PreviewRect {
                x: other.x - rect.width,
                y: snap_orthogonal(rect.y, other.y, other.bottom() - rect.height, threshold),
                ..rect
            },
            PreviewRect {
                x: other.right(),
                y: snap_orthogonal(rect.y, other.y, other.bottom() - rect.height, threshold),
                ..rect
            },
        ];
        let vertical_candidates = [
            PreviewRect {
                x: snap_orthogonal(rect.x, other.x, other.right() - rect.width, threshold),
                y: other.y - rect.height,
                ..rect
            },
            PreviewRect {
                x: snap_orthogonal(rect.x, other.x, other.right() - rect.width, threshold),
                y: other.bottom(),
                ..rect
            },
        ];

        for candidate in horizontal_candidates
            .into_iter()
            .chain(vertical_candidates.into_iter())
        {
            let distance = (candidate.x - rect.x).abs() + (candidate.y - rect.y).abs();
            if best.is_none_or(|(best_distance, _)| distance < best_distance) {
                best = Some((distance, candidate));
            }
        }
    }

    best.map(|(_, candidate)| candidate).unwrap_or(rect)
}

fn snap_orthogonal(value: i32, edge_a: i32, edge_b: i32, threshold: i32) -> i32 {
    if (value - edge_a).abs() <= threshold {
        edge_a
    } else if (value - edge_b).abs() <= threshold {
        edge_b
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        CompositorKind, DisplayDraft, DisplayOutput, DisplaySnapshot, layout_boxes,
        managed_displays_path, managed_include_path,
    };

    #[test]
    fn niri_managed_path_uses_glimpse_directory() {
        assert_eq!(
            managed_displays_path(CompositorKind::Niri),
            "~/.config/niri/glimpse.d/displays.kdl"
        );
    }

    #[test]
    fn hyprland_managed_path_uses_glimpse_directory() {
        assert_eq!(
            managed_displays_path(CompositorKind::Hyprland),
            "~/.config/hypr/glimpse.d/displays.conf"
        );
    }

    #[test]
    fn niri_managed_include_path_uses_index_file() {
        assert_eq!(
            managed_include_path(CompositorKind::Niri),
            "~/.config/niri/glimpse.d/index.kdl"
        );
    }

    #[test]
    fn niri_compositor_metadata_strings_match_current_pipeline() {
        assert_eq!(
            CompositorKind::Niri.live_source_label(),
            "niri msg --json outputs"
        );
        assert_eq!(CompositorKind::Niri.validation_label(), "niri validate -c");
        assert_eq!(
            CompositorKind::Niri.reload_method_label(),
            "niri msg action load-config-file"
        );
    }

    #[test]
    fn niri_json_derives_vrr_support_from_output_data() {
        let snapshot = super::snapshot_from_niri_json(
            r#"{
                "DP-2": {
                    "name": "DP-2",
                    "make": "Dell Inc.",
                    "model": "AW2725Q",
                    "serial": "69QC174",
                    "modes": [
                        {"width": 3840, "height": 2160, "refresh_rate": 59997, "is_preferred": true}
                    ],
                    "current_mode": 0,
                    "is_custom_mode": false,
                    "vrr_supported": true,
                    "vrr_enabled": true,
                    "logical": {"x": 0, "y": 0, "width": 3072, "height": 1728, "scale": 1.25, "transform": "Normal"}
                }
            }"#,
        )
        .expect("niri json should parse");

        assert_eq!(snapshot.outputs[0].vrr_enabled, Some(true));
        assert_eq!(snapshot.outputs[0].hdr_enabled, None);
        assert_eq!(snapshot.outputs[0].ten_bit_enabled, None);
    }

    #[test]
    fn layout_boxes_preserve_relative_positions() {
        let boxes = layout_boxes(
            &[
                DisplayOutput::test("eDP-1", 0, 0, 1920, 1200),
                DisplayOutput::test("DP-1", 1920, 120, 2560, 1440),
            ],
            640,
            220,
            24,
        );

        assert_eq!(boxes.len(), 2);
        assert!(boxes[1].x > boxes[0].x);
        assert!(boxes[1].y > boxes[0].y);
        assert!(boxes[0].width > 0);
        assert!(boxes[1].height > 0);
    }

    #[test]
    fn layout_scene_does_not_overlap_boxes_that_only_touch_edges() {
        let boxes = layout_boxes(
            &[
                DisplayOutput::test("DP-2", 0, 0, 3072, 1728),
                DisplayOutput::test("eDP-1", 3072, 0, 2880, 1800),
            ],
            720,
            220,
            18,
        );

        assert_eq!(boxes.len(), 2);
        assert!(boxes[0].x + boxes[0].width <= boxes[1].x);
    }

    #[test]
    fn layout_scene_uses_logical_size_derived_from_scale() {
        let mut external = DisplayOutput::test("DP-2", -3072, 0, 3072, 1728);
        external.scale = 1.25;
        external.current_mode = super::DisplayMode {
            width: 3840,
            height: 2160,
            refresh_millihz: 239_991,
            preferred: true,
        };
        external.sync_logical_geometry();
        let laptop = DisplayOutput::test("eDP-1", 0, 0, 2880, 1800);

        let scene = super::layout_scene(&[external, laptop], 720, 240, 18);

        assert_eq!(scene.boxes.len(), 2);
        assert!(scene.boxes[0].width > scene.boxes[1].width);
        assert!(scene.boxes[0].height < scene.boxes[1].height);
    }

    #[test]
    fn compositor_detection_prefers_niri_markers() {
        let compositor = CompositorKind::from_env([
            ("XDG_CURRENT_DESKTOP", "niri"),
            ("DESKTOP_SESSION", "niri-uwsm"),
            ("NIRI_SOCKET", "/tmp/niri.sock"),
        ]);

        assert_eq!(compositor, CompositorKind::Niri);
    }

    #[test]
    fn niri_json_preserves_connected_but_disabled_outputs() {
        let snapshot = super::snapshot_from_niri_json(
            r#"{
                "DP-2": {
                    "name": "DP-2",
                    "make": "Dell Inc.",
                    "model": "AW2725Q",
                    "serial": "69QC174",
                    "modes": [
                        {"width": 3840, "height": 2160, "refresh_rate": 59997, "is_preferred": true},
                        {"width": 3840, "height": 2160, "refresh_rate": 239991, "is_preferred": false}
                    ],
                    "current_mode": 1,
                    "is_custom_mode": false,
                    "vrr_supported": true,
                    "vrr_enabled": false,
                    "logical": {"x": -3072, "y": 0, "width": 3072, "height": 1728, "scale": 1.25, "transform": "Normal"}
                },
                "eDP-1": {
                    "name": "eDP-1",
                    "make": "Samsung Display Corp.",
                    "model": "ATNA60CL10-0 ",
                    "serial": null,
                    "physical_size": [340, 220],
                    "modes": [
                        {"width": 2880, "height": 1800, "refresh_rate": 120000, "is_preferred": true},
                        {"width": 2880, "height": 1800, "refresh_rate": 60001, "is_preferred": false}
                    ],
                    "current_mode": null,
                    "is_custom_mode": false,
                    "vrr_supported": true,
                    "vrr_enabled": false,
                    "logical": null
                }
            }"#,
        )
        .expect("niri json should parse");

        assert_eq!(snapshot.outputs.len(), 2);
        assert!(
            snapshot
                .outputs
                .iter()
                .any(|output| output.id == "DP-2" && output.enabled)
        );
        assert!(
            snapshot
                .outputs
                .iter()
                .any(|output| output.id == "eDP-1" && !output.enabled)
        );
        let internal = snapshot
            .outputs
            .iter()
            .find(|output| output.id == "eDP-1")
            .unwrap();
        assert_eq!(internal.connector, "eDP-1");
        assert_eq!(internal.make.as_deref(), Some("Samsung Display Corp."));
        assert_eq!(internal.model.as_deref(), Some("ATNA60CL10-0 "));
        assert_eq!(internal.serial, None);
        assert_eq!(internal.physical_size_mm, Some((340, 220)));
    }

    #[test]
    fn parses_edid_decode_panel_and_hdr_capabilities() {
        let info = super::parse_edid_decode(
            r#"
            Block 0, Base EDID:
              Vendor & Product Identification:
                Made in: 2023
              Basic Display Parameters & Features:
                Bits per primary color channel: 10
            Block 1, DisplayID Extension Block:
              Display Parameters Data Block (0x21):
                Native Color Depth: 12 bpc
                Display Device Technology: Organic LED
              Display Product Type: Standalone display device
              Display Interface Features Data Block:
                Supported bpc for RGB encoding: 6, 8, 10
                Supported bpc for YCbCr 4:4:4 encoding: 8, 10
                Supported color space and EOTF standard combination 1: DCI-P3, BT.2020/SMPTE ST 2084
              Colorimetry Data Block:
                BT2020RGB
              HDR Static Metadata Data Block:
                Electro optical transfer functions:
                  Traditional gamma - SDR luminance range
                  SMPTE ST2084
            "#,
        );

        assert_eq!(
            info.display_class.as_deref(),
            Some("Standalone display device")
        );
        assert_eq!(info.manufacture_date.as_deref(), Some("2023"));
        assert_eq!(info.bits_per_primary_channel.as_deref(), Some("10 bpc"));
        assert_eq!(info.native_color_depth.as_deref(), Some("12 bpc"));
        assert_eq!(info.panel_technology.as_deref(), Some("Organic LED"));
        assert!(
            info.supported_transport_depths
                .as_deref()
                .unwrap()
                .contains("Supported bpc for RGB encoding: 6, 8, 10")
        );
        let color_caps = info.color_capabilities.as_deref().unwrap();
        assert!(color_caps.contains("DCI-P3, BT.2020/SMPTE ST 2084"));
        assert!(color_caps.contains("BT2020RGB"));
        let hdr_caps = info.hdr_capabilities.as_deref().unwrap();
        assert!(hdr_caps.contains("HDR Static Metadata"));
        assert!(hdr_caps.contains("SMPTE ST2084"));
    }

    #[test]
    fn parses_edid_decode_supported_input_formats() {
        let info = super::parse_edid_decode(
            r#"
            Block 0, Base EDID:
              Basic Display Parameters & Features:
                Bits per primary color channel: 10
                Supported color formats: RGB 4:4:4, YCrCb 4:4:4, YCrCb 4:2:2
            "#,
        );

        assert_eq!(
            info.supported_input_formats.as_deref(),
            Some("RGB 4:4:4, YCrCb 4:4:4, YCrCb 4:2:2")
        );
        assert_eq!(info.bits_per_primary_channel.as_deref(), Some("10 bpc"));
    }

    #[test]
    fn niri_json_uses_preferred_mode_for_disabled_output_dimensions() {
        let snapshot = super::snapshot_from_niri_json(
            r#"{
                "eDP-1": {
                    "name": "eDP-1",
                    "make": "Samsung Display Corp.",
                    "model": "ATNA60CL10-0 ",
                    "serial": null,
                    "modes": [
                        {"width": 2880, "height": 1800, "refresh_rate": 120000, "is_preferred": true},
                        {"width": 1920, "height": 1200, "refresh_rate": 60000, "is_preferred": false}
                    ],
                    "current_mode": null,
                    "is_custom_mode": false,
                    "vrr_supported": true,
                    "vrr_enabled": false,
                    "logical": null
                }
            }"#,
        )
        .expect("niri json should parse");

        let output = &snapshot.outputs[0];
        assert_eq!(output.width, 2880);
        assert_eq!(output.height, 1800);
        assert_eq!(output.scale, 1.0);
        assert_eq!(output.current_mode.refresh_millihz, 120_000);
    }

    #[test]
    fn layout_boxes_keep_disabled_outputs_visible() {
        let mut outputs = vec![
            DisplayOutput::test("DP-2", 0, 0, 3072, 1728),
            DisplayOutput::test("eDP-1", 3096, 120, 2880, 1800),
        ];
        outputs[1].enabled = false;

        let boxes = layout_boxes(&outputs, 720, 240, 18);

        assert_eq!(boxes.len(), 2);
    }

    #[test]
    fn draft_moves_selected_output() {
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![
                DisplayOutput::test("DP-2", 0, 0, 3072, 1728),
                DisplayOutput::test("eDP-1", 3096, 120, 2880, 1800),
            ],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);
        draft.select_output("eDP-1");

        draft.move_output_by("eDP-1", 100, -40);

        let output = draft.selected_output().unwrap();
        assert_eq!(output.x, 3196);
        assert_eq!(output.y, 80);
    }

    #[test]
    fn draft_updates_boolean_toggles_for_selected_output() {
        let mut primary = DisplayOutput::test("eDP-1", 0, 0, 1920, 1080);
        primary.primary = true;
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Hyprland,
            outputs: vec![
                primary,
                DisplayOutput {
                    hdr_enabled: Some(false),
                    ten_bit_enabled: Some(false),
                    ..DisplayOutput::test("DP-2", 1920, 0, 3072, 1728)
                },
            ],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);
        draft.select_output("DP-2");

        draft.set_selected_enabled(false);
        draft.set_selected_vrr_enabled(true);
        draft.set_selected_hdr_enabled(true);
        draft.set_selected_ten_bit_enabled(true);

        let output = draft.selected_output().unwrap();
        assert!(!output.enabled);
        assert_eq!(output.vrr_enabled, Some(true));
        assert_eq!(output.hdr_enabled, Some(true));
        assert_eq!(output.ten_bit_enabled, Some(true));
    }

    #[test]
    fn draft_allows_invalid_state_until_apply_validation() {
        let mut only = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        only.primary = true;
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![only],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);

        draft.set_selected_enabled(false);

        let output = draft.selected_output().unwrap();
        assert!(!output.enabled);
        assert_eq!(
            draft.validate().as_ref().map_err(|error| error.as_str()),
            Err("at least one display must remain enabled")
        );
    }

    #[test]
    fn apply_rejects_draft_with_no_enabled_outputs() {
        let mut only = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        only.primary = true;
        only.enabled = false;
        let draft = DisplayDraft {
            compositor: CompositorKind::Niri,
            outputs: vec![only],
            selected_output_id: Some("DP-2".into()),
            mirror: false,
        };

        let error = super::apply_persisted_displays(&draft).expect_err("apply should fail");

        assert!(error.contains("at least one display must remain enabled"));
    }

    #[test]
    fn draft_updates_mode_and_scale_for_selected_output() {
        let mut output = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        output.available_modes = vec![
            output.current_mode.clone(),
            super::DisplayMode {
                width: 3840,
                height: 2160,
                refresh_millihz: 240_000,
                preferred: false,
            },
        ];
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![output],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);

        draft.set_selected_mode_index(1);
        draft.set_selected_scale(1.25);

        let output = draft.selected_output().unwrap();
        assert_eq!(output.current_mode.width, 3840);
        assert_eq!(output.current_mode.refresh_millihz, 240_000);
        assert_eq!(output.scale, 1.25);
        assert_eq!(output.width, 3072);
        assert_eq!(output.height, 1728);
    }

    #[test]
    fn changing_selected_scale_realigns_outputs_attached_on_the_right() {
        let mut primary = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        primary.primary = true;
        primary.scale = 1.25;
        primary.current_mode = super::DisplayMode {
            width: 3840,
            height: 2160,
            refresh_millihz: 239_991,
            preferred: true,
        };
        primary.sync_logical_geometry();

        let secondary = DisplayOutput::test("eDP-1", 3072, 0, 2880, 1800);
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![primary, secondary],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);

        draft.set_selected_scale(1.0);

        let primary = draft.outputs.iter().find(|output| output.primary).unwrap();
        let secondary = draft
            .outputs
            .iter()
            .find(|output| output.id == "eDP-1")
            .unwrap();
        assert_eq!(primary.width, 3840);
        assert_eq!(secondary.x, 3840);
        assert_eq!(secondary.y, 0);
    }

    #[test]
    fn draft_updates_orientation_and_marks_dirty() {
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![DisplayOutput::test("DP-2", 0, 0, 3072, 1728)],
        };
        let baseline = DisplayDraft::from_snapshot(snapshot.clone());
        let mut draft = DisplayDraft::from_snapshot(snapshot);

        draft.set_selected_orientation(super::DisplayOrientation::PortraitRight);

        let output = draft.selected_output().unwrap();
        assert_eq!(output.orientation, super::DisplayOrientation::PortraitRight);
        assert_eq!(output.width, 1728);
        assert_eq!(output.height, 3072);
        assert!(draft.is_dirty_against(&baseline));
    }

    #[test]
    fn draft_snaps_output_to_neighbor_edges() {
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![
                DisplayOutput::test("DP-2", 0, 0, 3072, 1728),
                DisplayOutput::test("eDP-1", 3072, 0, 2880, 1800),
            ],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);

        draft.move_output_by_preview_delta("eDP-1", -30.0, 12.0, 48.0);

        let output = draft
            .outputs
            .iter()
            .find(|output| output.id == "eDP-1")
            .unwrap();
        assert_eq!(output.x, 3072);
        assert_eq!(output.y, 0);
    }

    #[test]
    fn draft_can_snap_output_above_neighbor_for_stacking() {
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![
                DisplayOutput::test("DP-2", 0, 0, 3072, 1728),
                DisplayOutput::test("eDP-1", 3072, 0, 2880, 1800),
            ],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);

        draft.move_output_by_preview_delta("eDP-1", -3100.0, -1900.0, 48.0);

        let output = draft
            .outputs
            .iter()
            .find(|output| output.id == "eDP-1")
            .unwrap();
        assert_eq!(output.x, 0);
        assert_eq!(output.y, -1800);
    }

    #[test]
    fn draft_can_swap_output_to_other_side_of_neighbor() {
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![
                DisplayOutput::test("DP-2", 0, 0, 3072, 1728),
                DisplayOutput::test("eDP-1", 3072, 0, 2880, 1800),
            ],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);

        draft.move_output_by_preview_delta("DP-2", 5000.0, 0.0, 48.0);

        let output = draft
            .outputs
            .iter()
            .find(|output| output.id == "DP-2")
            .unwrap();
        assert_eq!(output.x, 5952);
        assert_eq!(output.y, 0);
    }

    #[test]
    fn dragging_primary_keeps_primary_origin_at_zero() {
        let mut primary = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        primary.primary = true;
        let secondary = DisplayOutput::test("eDP-1", 3072, 0, 2880, 1800);
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![primary, secondary],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);

        draft.move_output_by_preview_delta("DP-2", 120.0, 0.0, 48.0);

        let primary = draft.outputs.iter().find(|output| output.primary).unwrap();
        assert_eq!(primary.x, 0);
        assert_eq!(primary.y, 0);
    }

    #[test]
    fn draft_sets_primary_output_exclusively() {
        let mut left = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        left.primary = true;
        let right = DisplayOutput::test("eDP-1", 3072, 0, 2880, 1800);
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![left, right],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);

        draft.set_primary_output("eDP-1");

        assert_eq!(draft.selected_output_id.as_deref(), Some("eDP-1"));
        assert!(!draft.outputs[0].primary);
        assert!(draft.outputs[1].primary);
    }

    #[test]
    fn draft_can_place_selected_relative_to_primary() {
        let mut primary = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        primary.primary = true;
        let secondary = DisplayOutput::test("eDP-1", 3072, 0, 2880, 1800);
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![primary, secondary],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);
        draft.select_output("eDP-1");

        draft.place_selected_relative_to_primary(super::DisplayPlacement::Top);

        let output = draft
            .outputs
            .iter()
            .find(|output| output.id == "eDP-1")
            .unwrap();
        assert_eq!(output.x, 0);
        assert_eq!(output.y, -1800);
    }

    #[test]
    fn draft_places_relative_to_primary_using_logical_sizes() {
        let mut primary = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        primary.primary = true;
        primary.scale = 2.0;
        primary.current_mode = super::DisplayMode {
            width: 3840,
            height: 2160,
            refresh_millihz: 60_000,
            preferred: true,
        };
        primary.sync_logical_geometry();

        let secondary = DisplayOutput::test("HDMI-A-1", 1920, 0, 1920, 1080);
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![primary, secondary],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);
        draft.select_output("HDMI-A-1");

        draft.place_selected_relative_to_primary(super::DisplayPlacement::Right);

        let output = draft
            .outputs
            .iter()
            .find(|output| output.id == "HDMI-A-1")
            .unwrap();
        assert_eq!(output.x, 1920);
        assert_eq!(output.y, 0);
    }

    #[test]
    fn preset_places_selected_into_non_overlapping_right_stack() {
        let mut primary = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        primary.primary = true;
        let right_top = DisplayOutput::test("HDMI-A-1", 3072, 0, 1920, 1080);
        let selected = DisplayOutput::test("eDP-1", -2880, 0, 2880, 1800);
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![primary, right_top, selected],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);
        draft.select_output("eDP-1");

        draft.place_selected_relative_to_primary(super::DisplayPlacement::Right);

        let output = draft
            .outputs
            .iter()
            .find(|output| output.id == "eDP-1")
            .unwrap();
        assert_eq!(output.x, 3072);
        assert_eq!(output.y, 1080);
    }

    #[test]
    fn preset_places_selected_into_non_overlapping_bottom_stack() {
        let mut primary = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        primary.primary = true;
        let bottom_left = DisplayOutput::test("HDMI-A-1", 0, 1728, 1920, 1080);
        let selected = DisplayOutput::test("eDP-1", 0, -1800, 2880, 1800);
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![primary, bottom_left, selected],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);
        draft.select_output("eDP-1");

        draft.place_selected_relative_to_primary(super::DisplayPlacement::Bottom);

        let output = draft
            .outputs
            .iter()
            .find(|output| output.id == "eDP-1")
            .unwrap();
        assert_eq!(output.x, 1920);
        assert_eq!(output.y, 1728);
    }

    #[test]
    fn external_snapshot_updates_only_baseline_while_draft_is_dirty() {
        let mut primary = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        primary.primary = true;
        let secondary = DisplayOutput::test("eDP-1", 3072, 0, 2880, 1800);
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![primary.clone(), secondary.clone()],
        };
        let mut baseline = DisplayDraft::from_snapshot(snapshot.clone());
        let mut draft = DisplayDraft::from_snapshot(snapshot);
        draft.select_output("eDP-1");
        draft.move_output_by("eDP-1", 64, 0);

        let fresh_snapshot = DisplaySnapshot {
            compositor: CompositorKind::Niri,
            outputs: vec![primary, DisplayOutput::test("eDP-1", 3840, 0, 2880, 1800)],
        };

        let outcome = super::reconcile_external_snapshot(&mut draft, &mut baseline, fresh_snapshot);

        assert_eq!(outcome, super::ExternalSnapshotUpdate::BaselineUpdated);
        let baseline_secondary = baseline
            .outputs
            .iter()
            .find(|output| output.id == "eDP-1")
            .unwrap();
        let draft_secondary = draft
            .outputs
            .iter()
            .find(|output| output.id == "eDP-1")
            .unwrap();
        assert_eq!(baseline_secondary.x, 3840);
        assert_eq!(draft_secondary.x, 3136);
    }

    #[test]
    fn niri_json_reads_orientation_from_transform() {
        let snapshot = super::snapshot_from_niri_json(
            r#"{
                "DP-2": {
                    "name": "DP-2",
                    "make": "Dell Inc.",
                    "model": "AW2725Q",
                    "serial": "69QC174",
                    "modes": [
                        {"width": 3840, "height": 2160, "refresh_rate": 59997, "is_preferred": true}
                    ],
                    "current_mode": 0,
                    "is_custom_mode": false,
                    "vrr_supported": true,
                    "vrr_enabled": false,
                    "logical": {"x": 0, "y": 0, "width": 2160, "height": 3840, "scale": 1.0, "transform": "90"}
                }
            }"#,
        )
        .expect("niri json should parse");

        assert_eq!(
            snapshot.outputs[0].orientation,
            super::DisplayOrientation::PortraitRight
        );
    }

    #[test]
    fn hypr_orientation_uses_numeric_transform_mapping() {
        assert_eq!(
            super::DisplayOrientation::from_hypr_transform(0),
            super::DisplayOrientation::Landscape
        );
        assert_eq!(
            super::DisplayOrientation::from_hypr_transform(1),
            super::DisplayOrientation::PortraitRight
        );
        assert_eq!(
            super::DisplayOrientation::from_hypr_transform(2),
            super::DisplayOrientation::UpsideDown
        );
        assert_eq!(
            super::DisplayOrientation::from_hypr_transform(3),
            super::DisplayOrientation::PortraitLeft
        );
        assert_eq!(super::DisplayOrientation::Landscape.hypr_transform(), 0);
        assert_eq!(super::DisplayOrientation::PortraitRight.hypr_transform(), 1);
        assert_eq!(super::DisplayOrientation::UpsideDown.hypr_transform(), 2);
        assert_eq!(super::DisplayOrientation::PortraitLeft.hypr_transform(), 3);
    }

    #[test]
    fn hypr_json_parses_mirror_and_ten_bit_state() {
        let snapshot = super::snapshot_from_hypr_json(
            r#"[
                {
                    "id": 1,
                    "name": "DP-2",
                    "description": "Dell Inc. AW2725Q (DP-2)",
                    "make": "Dell Inc.",
                    "model": "AW2725Q",
                    "serial": "69QC174",
                    "width": 3840,
                    "height": 2160,
                    "refreshRate": 239.991,
                    "x": 0,
                    "y": 0,
                    "scale": 1.25,
                    "transform": 0,
                    "focused": true,
                    "disabled": false,
                    "dpmsStatus": true,
                    "vrr": true,
                    "currentFormat": "XBGR2101010",
                    "mirrorOf": "none",
                    "availableModes": ["3840x2160@239.991Hz", "3840x2160@59.997Hz"]
                },
                {
                    "id": 2,
                    "name": "HDMI-A-1",
                    "description": "LG Electronics LG TV (HDMI-A-1)",
                    "make": "LG Electronics",
                    "model": "LG TV",
                    "serial": "",
                    "width": 1920,
                    "height": 1080,
                    "refreshRate": 60.000,
                    "x": 3072,
                    "y": 0,
                    "scale": 1.0,
                    "transform": 0,
                    "focused": false,
                    "disabled": false,
                    "dpmsStatus": true,
                    "vrr": false,
                    "currentFormat": "XRGB8888",
                    "mirrorOf": "DP-2",
                    "availableModes": ["1920x1080@60.000Hz"]
                },
                {
                    "id": 3,
                    "name": "eDP-1",
                    "description": "Samsung Display Corp. ATNA60CL10-0 (eDP-1)",
                    "make": "Samsung Display Corp.",
                    "model": "ATNA60CL10-0",
                    "serial": "",
                    "width": 2880,
                    "height": 1800,
                    "refreshRate": 120.000,
                    "x": 0,
                    "y": 0,
                    "scale": 1.0,
                    "transform": 0,
                    "focused": false,
                    "disabled": true,
                    "dpmsStatus": false,
                    "vrr": false,
                    "currentFormat": "XRGB8888",
                    "mirrorOf": "none",
                    "availableModes": ["2880x1800@120.000Hz", "2880x1800@60.001Hz"]
                }
            ]"#,
            &std::collections::HashMap::new(),
        )
        .expect("hypr json should parse");

        assert_eq!(snapshot.compositor, CompositorKind::Hyprland);
        assert_eq!(snapshot.outputs.len(), 3);
        let primary = snapshot
            .outputs
            .iter()
            .find(|output| output.id == "DP-2")
            .unwrap();
        assert!(primary.primary);
        assert_eq!(primary.ten_bit_enabled, Some(true));
        assert_eq!(primary.vrr_enabled, Some(true));
        let mirrored = snapshot
            .outputs
            .iter()
            .find(|output| output.id == "HDMI-A-1")
            .unwrap();
        assert_eq!(mirrored.mirror_source.as_deref(), Some("DP-2"));
        let disabled = snapshot
            .outputs
            .iter()
            .find(|output| output.id == "eDP-1")
            .unwrap();
        assert!(!disabled.enabled);
        assert_eq!(disabled.current_mode.width, 2880);
        assert_eq!(disabled.current_mode.height, 1800);
    }

    #[test]
    fn hypr_managed_state_overlays_hdr_and_mirror_settings() {
        let state = super::parse_hypr_managed_state(
            r#"
            # generated by glimpse
            monitor = DP-2, 3840x2160@239.991, 0x0, 1.25, bitdepth, 10, cm, hdr
            monitor = HDMI-A-1, 1920x1080@60.000, 3072x0, 1.00, mirror, DP-2
            monitor = eDP-1, disable
            "#,
        );

        let primary = state.get("DP-2").expect("primary state should exist");
        assert_eq!(primary.ten_bit_enabled, Some(true));
        assert_eq!(primary.hdr_enabled, Some(true));
        let mirrored = state.get("HDMI-A-1").expect("mirror state should exist");
        assert_eq!(mirrored.mirror_source.as_deref(), Some("DP-2"));
        let disabled = state.get("eDP-1").expect("disabled state should exist");
        assert_eq!(disabled.enabled, Some(false));
    }

    #[test]
    fn draft_updates_mirror_source_for_selected_output() {
        let mut primary = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        primary.primary = true;
        let secondary = DisplayOutput::test("HDMI-A-1", 3072, 0, 1920, 1080);
        let snapshot = DisplaySnapshot {
            compositor: CompositorKind::Hyprland,
            outputs: vec![primary, secondary],
        };
        let mut draft = DisplayDraft::from_snapshot(snapshot);
        draft.select_output("HDMI-A-1");

        draft.set_selected_mirror_source(Some("DP-2"));

        let mirrored = draft
            .outputs
            .iter()
            .find(|output| output.id == "HDMI-A-1")
            .unwrap();
        assert_eq!(mirrored.mirror_source.as_deref(), Some("DP-2"));
    }

    #[test]
    fn serializes_hypr_outputs_with_mirror_hdr_and_ten_bit() {
        let mut primary = DisplayOutput::test("DP-2", 0, 0, 3072, 1728);
        primary.primary = true;
        primary.scale = 1.25;
        primary.current_mode = super::DisplayMode {
            width: 3840,
            height: 2160,
            refresh_millihz: 239_991,
            preferred: true,
        };
        primary.sync_logical_geometry();
        primary.vrr_enabled = None;
        primary.hdr_enabled = Some(true);
        primary.ten_bit_enabled = Some(true);

        let mut mirrored = DisplayOutput::test("HDMI-A-1", 3072, 0, 1920, 1080);
        mirrored.vrr_enabled = None;
        mirrored.mirror_source = Some("DP-2".into());

        let mut disabled = DisplayOutput::test("eDP-1", 4992, 0, 2880, 1800);
        disabled.enabled = false;

        let draft = DisplayDraft {
            compositor: CompositorKind::Hyprland,
            outputs: vec![primary, mirrored, disabled],
            selected_output_id: Some("DP-2".into()),
            mirror: false,
        };

        let text = super::serialize_hypr_draft(&draft);

        assert!(
            text.contains("monitor = DP-2, 3840x2160@239.991, 0x0, 1.25, bitdepth, 10, cm, hdr")
        );
        assert!(text.contains("monitor = HDMI-A-1, 1920x1080@60.000, 3072x0, 1.00, mirror, DP-2"));
        assert!(text.contains("monitor = eDP-1, disable"));
    }

    #[test]
    fn serializes_niri_outputs_into_managed_fragment() {
        let mut off = DisplayOutput::test("eDP-1", 0, 0, 2880, 1800);
        off.enabled = false;
        off.available_modes = vec![super::DisplayMode {
            width: 2880,
            height: 1800,
            refresh_millihz: 120_000,
            preferred: true,
        }];

        let on = DisplayOutput {
            id: "DP-2".into(),
            title: "Dell".into(),
            connector: "DP-2".into(),
            make: None,
            model: None,
            serial: None,
            physical_size_mm: None,
            edid: None,
            enabled: true,
            primary: true,
            x: -3072,
            y: 0,
            width: 3072,
            height: 1728,
            scale: 1.25,
            orientation: super::DisplayOrientation::Landscape,
            current_mode: super::DisplayMode {
                width: 3840,
                height: 2160,
                refresh_millihz: 239_991,
                preferred: false,
            },
            available_modes: vec![],
            vrr_enabled: Some(true),
            hdr_enabled: None,
            ten_bit_enabled: None,
            mirror_source: None,
        };
        let draft = DisplayDraft {
            compositor: CompositorKind::Niri,
            outputs: vec![on, off],
            selected_output_id: Some("DP-2".into()),
            mirror: false,
        };

        let text = super::serialize_niri_draft(&draft);

        assert!(text.contains("output \"DP-2\""));
        assert!(text.contains("mode \"3840x2160@239.991\""));
        assert!(text.contains("position x=-3072 y=0"));
        assert!(text.contains("scale 1.25"));
        assert!(text.contains("transform \"normal\""));
        assert!(text.contains("variable-refresh-rate"));
        assert!(text.contains("focus-at-startup"));
        assert!(text.contains("output \"eDP-1\""));
        assert!(text.contains("off"));
    }

    #[test]
    fn serializer_emits_primary_at_zero_origin() {
        let primary = DisplayOutput {
            id: "DP-2".into(),
            title: "Dell".into(),
            connector: "DP-2".into(),
            make: None,
            model: None,
            serial: None,
            physical_size_mm: None,
            edid: None,
            enabled: true,
            primary: true,
            x: -4328,
            y: 0,
            width: 3072,
            height: 1728,
            scale: 1.25,
            orientation: super::DisplayOrientation::Landscape,
            current_mode: super::DisplayMode {
                width: 3840,
                height: 2160,
                refresh_millihz: 239_991,
                preferred: false,
            },
            available_modes: vec![],
            vrr_enabled: Some(true),
            hdr_enabled: None,
            ten_bit_enabled: None,
            mirror_source: None,
        };
        let secondary = DisplayOutput::test("eDP-1", -1570, 0, 2880, 1800);
        let mut draft = DisplayDraft {
            compositor: CompositorKind::Niri,
            outputs: vec![primary, secondary],
            selected_output_id: Some("DP-2".into()),
            mirror: false,
        };

        draft.normalize_to_primary_origin();
        let text = super::serialize_niri_draft(&draft);

        assert!(
            text.contains(
                "output \"DP-2\" {\n    mode \"3840x2160@239.991\"\n    position x=0 y=0"
            )
        );
    }

    #[test]
    fn finalize_niri_apply_reloads_when_include_is_present() {
        let status = super::finalize_niri_apply(
            PathBuf::from("/tmp/displays.kdl"),
            true,
            |_| Ok(()),
            || Ok(()),
        )
        .expect("apply should succeed");

        assert_eq!(
            status,
            super::PersistStatus::Applied {
                path: PathBuf::from("/tmp/displays.kdl"),
                include_present: true,
                reloaded: true,
            }
        );
    }

    #[test]
    fn finalize_niri_apply_skips_reload_without_include() {
        let mut reloaded = false;
        let status = super::finalize_niri_apply(
            PathBuf::from("/tmp/displays.kdl"),
            false,
            |_| Ok(()),
            || {
                reloaded = true;
                Ok(())
            },
        )
        .expect("apply should succeed");

        assert!(!reloaded);
        assert_eq!(
            status,
            super::PersistStatus::Applied {
                path: PathBuf::from("/tmp/displays.kdl"),
                include_present: false,
                reloaded: false,
            }
        );
    }

    #[test]
    fn finalize_niri_apply_validates_managed_fragment_before_reload() {
        let mut validated = false;
        let mut reloaded = false;

        let status = super::finalize_niri_apply(
            PathBuf::from("/tmp/displays.kdl"),
            true,
            |path| {
                validated = true;
                assert_eq!(path, Path::new("/tmp/displays.kdl"));
                Ok(())
            },
            || {
                reloaded = true;
                Ok(())
            },
        )
        .expect("apply should succeed");

        assert!(validated);
        assert!(reloaded);
        assert_eq!(
            status,
            super::PersistStatus::Applied {
                path: PathBuf::from("/tmp/displays.kdl"),
                include_present: true,
                reloaded: true,
            }
        );
    }

    #[test]
    fn finalize_niri_apply_stops_when_validation_fails() {
        let mut reloaded = false;

        let error = super::finalize_niri_apply(
            PathBuf::from("/tmp/displays.kdl"),
            true,
            |_| Err("bad config".into()),
            || {
                reloaded = true;
                Ok(())
            },
        )
        .expect_err("apply should fail");

        assert_eq!(error, "bad config");
        assert!(!reloaded);
    }

    #[test]
    fn serializes_niri_index_to_include_all_managed_fragments() {
        let text = crate::niri_managed::serialize_niri_index(&["backdrop.kdl", "displays.kdl"]);

        assert!(text.contains("include \"~/.config/niri/glimpse.d/displays.kdl\""));
        assert!(text.contains("include \"~/.config/niri/glimpse.d/backdrop.kdl\""));
    }
}
