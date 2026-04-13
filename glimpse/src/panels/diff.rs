use glimpse::config::{AppletConfig, PanelConfig, PanelPosition};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelKey {
    pub position: PanelPosition,
    pub ordinal: usize,
}

impl Hash for PanelKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.position {
            PanelPosition::Top => 0u8.hash(state),
            PanelPosition::Bottom => 1u8.hash(state),
        }
        self.ordinal.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelSection {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AppletInstanceKey {
    pub panel: PanelKey,
    pub section: PanelSection,
    pub slot_index: usize,
    pub configured_name: String,
    pub resolved_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionEntry {
    pub slot: usize,
    pub key: AppletInstanceKey,
    pub name: String,
    pub applet_type: String,
}

impl AppletInstanceKey {
    pub fn new(
        panel: &PanelKey,
        section: PanelSection,
        slot_index: usize,
        configured_name: impl Into<String>,
        resolved_type: impl Into<String>,
    ) -> Self {
        Self {
            panel: panel.clone(),
            section,
            slot_index,
            configured_name: configured_name.into(),
            resolved_type: resolved_type.into(),
        }
    }
}

pub fn build_panel_keys(configs: &[PanelConfig]) -> Vec<PanelKey> {
    let mut top_ordinal = 0;
    let mut bottom_ordinal = 0;

    configs
        .iter()
        .map(|config| {
            let ordinal = match &config.position {
                PanelPosition::Top => {
                    let ordinal = top_ordinal;
                    top_ordinal += 1;
                    ordinal
                }
                PanelPosition::Bottom => {
                    let ordinal = bottom_ordinal;
                    bottom_ordinal += 1;
                    ordinal
                }
            };

            PanelKey {
                position: config.position.clone(),
                ordinal,
            }
        })
        .collect()
}

pub fn build_section_entries(
    panel: &PanelKey,
    section: PanelSection,
    names: &[String],
    applet_configs: &HashMap<String, AppletConfig>,
) -> Vec<SectionEntry> {
    names.iter()
        .enumerate()
        .map(|(slot, name)| {
            let applet_type = applet_configs
                .get(name)
                .map(|config| config.extends.as_str())
                .filter(|value| !value.is_empty())
                .unwrap_or(name.as_str())
                .to_string();

            SectionEntry {
                slot,
                key: AppletInstanceKey::new(panel, section, slot, name, &applet_type),
                name: name.clone(),
                applet_type,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::config::{Margin, PanelConfig, PanelPosition};

    fn panel(position: PanelPosition) -> PanelConfig {
        PanelConfig {
            position,
            height: 36,
            margin: Margin::default(),
            left: Vec::new(),
            center: Vec::new(),
            right: Vec::new(),
        }
    }

    #[test]
    fn panel_keys_are_scoped_by_position_and_ordinal() {
        let configs = vec![
            panel(PanelPosition::Top),
            panel(PanelPosition::Top),
            panel(PanelPosition::Bottom),
        ];

        let keys = build_panel_keys(&configs);
        assert_eq!(
            keys[0],
            PanelKey {
                position: PanelPosition::Top,
                ordinal: 0,
            }
        );
        assert_eq!(
            keys[1],
            PanelKey {
                position: PanelPosition::Top,
                ordinal: 1,
            }
        );
        assert_eq!(
            keys[2],
            PanelKey {
                position: PanelPosition::Bottom,
                ordinal: 0,
            }
        );
    }

    #[test]
    fn duplicate_applet_names_get_distinct_instance_keys_by_slot() {
        let panel = PanelKey {
            position: PanelPosition::Top,
            ordinal: 0,
        };

        let first = AppletInstanceKey::new(&panel, PanelSection::Right, 0, "clock", "clock");
        let second = AppletInstanceKey::new(&panel, PanelSection::Right, 1, "clock", "clock");

        assert_ne!(first, second);
    }

    #[test]
    fn moving_applet_to_new_slot_changes_identity() {
        let panel = PanelKey {
            position: PanelPosition::Top,
            ordinal: 0,
        };

        let old = AppletInstanceKey::new(&panel, PanelSection::Left, 0, "network", "network");
        let new = AppletInstanceKey::new(&panel, PanelSection::Left, 1, "network", "network");

        assert_ne!(old, new);
    }

    #[test]
    fn section_entries_preserve_order_and_duplicates() {
        let panel = PanelKey {
            position: PanelPosition::Top,
            ordinal: 0,
        };
        let entries = build_section_entries(
            &panel,
            PanelSection::Right,
            &["clock".to_string(), "clock".to_string(), "tray".to_string()],
            &HashMap::new(),
        );

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].slot, 0);
        assert_eq!(entries[1].slot, 1);
        assert_eq!(entries[0].name, "clock");
        assert_eq!(entries[1].name, "clock");
        assert_eq!(entries[2].name, "tray");
        assert_ne!(entries[0].key, entries[1].key);
    }

    #[test]
    fn panel_list_keys_support_add_and_remove() {
        let old = vec![panel(PanelPosition::Top)];
        let new = vec![panel(PanelPosition::Top), panel(PanelPosition::Bottom)];

        let old_keys = build_panel_keys(&old);
        let new_keys = build_panel_keys(&new);

        assert_eq!(old_keys.len(), 1);
        assert_eq!(new_keys.len(), 2);
        assert!(new_keys.contains(&PanelKey {
            position: PanelPosition::Bottom,
            ordinal: 0,
        }));
    }
}
