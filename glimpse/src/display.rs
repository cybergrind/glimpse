use std::{
    fs,
    path::{Path, PathBuf},
};

use adw::gdk::{self, prelude::MonitorExt};

const SYS_DRM_DIR: &str = "/sys/class/drm";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayConnectorKind {
    Internal,
    External,
    Virtual,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrmConnectorState {
    pub connector: String,
    pub kind: DisplayConnectorKind,
    pub connected: bool,
    pub enabled: bool,
}

pub fn connector_kind(connector: &str) -> DisplayConnectorKind {
    let connector = normalize_connector_name(connector);
    let prefix = connector.split('-').next().unwrap_or(connector);

    match prefix {
        "eDP" | "LVDS" | "DSI" => DisplayConnectorKind::Internal,
        "DP" | "HDMI" | "DVI" | "VGA" => DisplayConnectorKind::External,
        "Virtual" | "Writeback" => DisplayConnectorKind::Virtual,
        _ => DisplayConnectorKind::Unknown,
    }
}

pub fn drm_connector_states() -> Vec<DrmConnectorState> {
    read_drm_connector_states(Path::new(SYS_DRM_DIR))
}

pub fn read_drm_connector_states(root: &Path) -> Vec<DrmConnectorState> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut connectors = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let connector = normalize_connector_name(name);
        if connector == name {
            continue;
        }

        let Some(connected) = read_flag(&path.join("status"), "connected") else {
            continue;
        };
        let enabled = read_flag(&path.join("enabled"), "enabled").unwrap_or(connected);

        connectors.push(DrmConnectorState {
            connector: connector.to_owned(),
            kind: connector_kind(connector),
            connected,
            enabled,
        });
    }

    connectors.sort_by(|left, right| left.connector.cmp(&right.connector));
    connectors
}

pub fn normalize_connector_name(name: &str) -> &str {
    if !name.starts_with("card") {
        return name;
    }

    let Some((_, connector)) = name.split_once('-') else {
        return name;
    };
    connector
}

pub fn connector_name(monitor: &gdk::Monitor) -> String {
    monitor
        .connector()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("monitor-{}", monitor.geometry().x()))
}

fn read_flag(path: &PathBuf, expected: &str) -> Option<bool> {
    Some(fs::read_to_string(path).ok()?.trim() == expected)
}

#[cfg(test)]
mod tests {
    use super::{DisplayConnectorKind, connector_kind};

    #[test]
    fn connector_kind_recognizes_internal_and_external_connectors() {
        assert_eq!(connector_kind("eDP-1"), DisplayConnectorKind::Internal);
        assert_eq!(
            connector_kind("card1-eDP-1"),
            DisplayConnectorKind::Internal
        );
        assert_eq!(connector_kind("DP-2"), DisplayConnectorKind::External);
        assert_eq!(connector_kind("HDMI-A-1"), DisplayConnectorKind::External);
        assert_eq!(connector_kind("Writeback-1"), DisplayConnectorKind::Virtual);
    }
}
