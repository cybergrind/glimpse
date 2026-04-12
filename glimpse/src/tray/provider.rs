use std::{path::Path, sync::Arc};

use anyhow::Context;
use system_tray::{
    client::{ActivateRequest, Client, Event},
    item::{Category, IconPixmap, Status, StatusNotifierItem, Tooltip},
    menu::{Disposition, MenuItem, MenuType, ToggleState, ToggleType, TrayMenu},
};
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

use crate::{
    dbus::status_notifier_item::StatusNotifierItemProxy,
    tray::protocol::{
        TrayCategory, TrayIcon, TrayItem, TrayMenuDisposition, TrayMenuItem, TrayMenuItemKind,
        TrayMenuToggleState, TrayMenuToggleType, TraySnapshot, TrayStatus, TrayTooltip,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayProviderEvent {
    Changed { reason: String },
}

#[derive(Clone)]
pub struct TrayProvider {
    client: Arc<Client>,
    session: zbus::Connection,
}

impl TrayProvider {
    pub async fn new() -> anyhow::Result<Self> {
        let session = zbus::Connection::session().await?;
        let client = Arc::new(Client::new().await?);
        Ok(Self { client, session })
    }

    pub async fn snapshot(&self) -> anyhow::Result<TraySnapshot> {
        let entries = {
            let map = self.client.items();
            let guard = map.lock().expect("tray provider map poisoned");
            guard
                .iter()
                .map(|(address, (item, menu))| (address.clone(), (item.clone(), menu.clone())))
                .collect::<Vec<_>>()
        };

        Ok(snapshot_from_entries(&entries))
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<TrayProviderEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let mut rx = self.client.subscribe();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                event = rx.recv() => {
                    match event {
                        Ok(event) => {
                            if events.send(TrayProviderEvent::Changed {
                                reason: event_reason(&event),
                            }).await.is_err() {
                                return Ok(());
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "tray provider: lagged behind tray event stream");
                        }
                        Err(broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
            }
        }
    }

    pub async fn activate(&self, address: String, x: i32, y: i32) -> anyhow::Result<()> {
        self.client
            .activate(ActivateRequest::Default { address, x, y })
            .await
            .map_err(Into::into)
    }

    pub async fn open_context_menu(&self, address: &str, x: i32, y: i32) -> anyhow::Result<()> {
        let (destination, path) = parse_address(address);
        let proxy = StatusNotifierItemProxy::builder(&self.session)
            .destination(destination.to_string())?
            .path(path)?
            .build()
            .await
            .context("failed to create tray item proxy")?;
        proxy.context_menu(x, y).await.map_err(Into::into)
    }

    pub async fn about_to_show_menu(
        &self,
        address: String,
        menu_path: String,
        item_id: i32,
    ) -> anyhow::Result<bool> {
        if menu_path.is_empty() {
            return Ok(false);
        }

        self.client
            .about_to_show_menuitem(address, menu_path, item_id)
            .await
            .map_err(Into::into)
    }

    pub async fn activate_menu_item(
        &self,
        address: String,
        menu_path: String,
        submenu_id: i32,
    ) -> anyhow::Result<()> {
        self.client
            .activate(ActivateRequest::MenuItem {
                address,
                menu_path,
                submenu_id,
            })
            .await
            .map_err(Into::into)
    }
}

pub fn map_item(address: &str, item: &StatusNotifierItem, menu: Option<&TrayMenu>) -> TrayItem {
    TrayItem {
        address: address.to_owned(),
        id: item.id.clone(),
        title: item.title.clone().unwrap_or_default(),
        status: map_status(item.status),
        category: map_category(item.category),
        item_is_menu: item.item_is_menu,
        menu_path: item.menu.clone().unwrap_or_default(),
        icon_theme_path: item.icon_theme_path.clone(),
        icon: map_icon(item.icon_name.as_deref(), item.icon_pixmap.as_deref()),
        overlay_icon: map_icon(
            item.overlay_icon_name.as_deref(),
            item.overlay_icon_pixmap.as_deref(),
        ),
        attention_icon: map_icon(
            item.attention_icon_name.as_deref(),
            item.attention_icon_pixmap.as_deref(),
        ),
        attention_movie_name: item.attention_movie_name.clone(),
        tooltip: map_tooltip(item.tool_tip.as_ref()),
        menu: menu
            .map(|menu| map_menu_items(&menu.submenus))
            .unwrap_or_default(),
    }
}

pub fn snapshot_from_entries(
    entries: &[(String, (StatusNotifierItem, Option<TrayMenu>))],
) -> TraySnapshot {
    let mut items = entries
        .iter()
        .map(|(address, (item, menu))| map_item(address, item, menu.as_ref()))
        .collect::<Vec<_>>();
    items.sort_by(|left, right| left.address.cmp(&right.address));
    TraySnapshot { items }
}

fn map_icon(name: Option<&str>, pixmaps: Option<&[IconPixmap]>) -> Option<TrayIcon> {
    if let Some(name) = name.filter(|value| !value.trim().is_empty()) {
        return Some(if Path::new(name).is_absolute() {
            TrayIcon::FilePath(name.to_owned())
        } else {
            TrayIcon::Name(name.to_owned())
        });
    }

    select_best_pixmap(pixmaps).map(|pixmap| TrayIcon::Pixmap {
        width: pixmap.width,
        height: pixmap.height,
        pixels: pixmap.pixels.clone(),
    })
}

fn select_best_pixmap(pixmaps: Option<&[IconPixmap]>) -> Option<&IconPixmap> {
    pixmaps?
        .iter()
        .max_by_key(|pixmap| i64::from(pixmap.width.max(0)) * i64::from(pixmap.height.max(0)))
}

fn map_tooltip(tooltip: Option<&Tooltip>) -> Option<TrayTooltip> {
    let tooltip = tooltip?;
    Some(TrayTooltip {
        title: tooltip.title.clone(),
        description: tooltip.description.clone(),
        icon: map_icon(
            Some(tooltip.icon_name.as_str()).filter(|value| !value.is_empty()),
            Some(tooltip.icon_data.as_slice()),
        ),
    })
}

fn map_menu_items(items: &[MenuItem]) -> Vec<TrayMenuItem> {
    items.iter().map(map_menu_item).collect()
}

fn map_menu_item(item: &MenuItem) -> TrayMenuItem {
    let kind = match item.menu_type {
        MenuType::Separator => TrayMenuItemKind::Separator,
        MenuType::Standard => TrayMenuItemKind::Standard,
    };
    let toggle_type = match item.toggle_type {
        ToggleType::Checkmark => TrayMenuToggleType::Checkmark,
        ToggleType::Radio => TrayMenuToggleType::Radio,
        ToggleType::CannotBeToggled => TrayMenuToggleType::CannotBeToggled,
    };
    let toggle_state = if matches!(toggle_type, TrayMenuToggleType::CannotBeToggled) {
        TrayMenuToggleState::Indeterminate
    } else {
        match item.toggle_state {
            ToggleState::On => TrayMenuToggleState::On,
            ToggleState::Off => TrayMenuToggleState::Off,
            ToggleState::Indeterminate => TrayMenuToggleState::Indeterminate,
        }
    };

    TrayMenuItem {
        id: item.id,
        label: sanitize_label(item.label.as_deref().unwrap_or_default()),
        enabled: matches!(kind, TrayMenuItemKind::Standard) && item.enabled,
        visible: item.visible || matches!(kind, TrayMenuItemKind::Separator),
        kind,
        icon: menu_item_icon(item),
        shortcut: item.shortcut.clone(),
        toggle_type,
        toggle_state,
        children_display: item.children_display.clone(),
        disposition: match item.disposition {
            Disposition::Normal => TrayMenuDisposition::Normal,
            Disposition::Informative => TrayMenuDisposition::Informative,
            Disposition::Warning => TrayMenuDisposition::Warning,
            Disposition::Alert => TrayMenuDisposition::Alert,
        },
        children: map_menu_items(&item.submenu),
    }
}

fn menu_item_icon(item: &MenuItem) -> Option<TrayIcon> {
    if let Some(icon) = map_icon(item.icon_name.as_deref(), None) {
        return Some(icon);
    }

    item.icon_data
        .as_ref()
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| TrayIcon::EncodedBytes(bytes.clone()))
}

fn sanitize_label(label: &str) -> String {
    label
        .replace("__", "\x00")
        .replace('_', "")
        .replace('\x00', "_")
}

fn map_category(category: Category) -> TrayCategory {
    match category {
        Category::ApplicationStatus => TrayCategory::ApplicationStatus,
        Category::Communications => TrayCategory::Communications,
        Category::SystemServices => TrayCategory::SystemServices,
        Category::Hardware => TrayCategory::Hardware,
    }
}

fn map_status(status: Status) -> TrayStatus {
    match status {
        Status::Unknown => TrayStatus::Unknown,
        Status::Passive => TrayStatus::Passive,
        Status::Active => TrayStatus::Active,
        Status::NeedsAttention => TrayStatus::NeedsAttention,
    }
}

fn event_reason(event: &Event) -> String {
    match event {
        Event::Add(address, _) => format!("add:{address}"),
        Event::Update(address, _) => format!("update:{address}"),
        Event::Remove(address) => format!("remove:{address}"),
    }
}

fn parse_address(address: &str) -> (&str, String) {
    address.split_once('/').map_or(
        (address, String::from("/StatusNotifierItem")),
        |(dest, path)| (dest, format!("/{path}")),
    )
}

#[cfg(test)]
mod tests {
    use system_tray::{
        item::{Category, IconPixmap, Status, StatusNotifierItem, Tooltip},
        menu::{Disposition, MenuItem, MenuType, ToggleState, ToggleType, TrayMenu},
    };

    use super::*;

    #[test]
    fn map_item_uses_absolute_paths_as_file_icons() {
        let item = test_item();
        let mapped = map_item(
            "org.example.App",
            &StatusNotifierItem {
                icon_name: Some("/tmp/example.png".into()),
                ..item
            },
            None,
        );

        assert_eq!(
            mapped.icon,
            Some(TrayIcon::FilePath("/tmp/example.png".into()))
        );
    }

    #[test]
    fn map_item_uses_pixmap_when_no_name_is_available() {
        let mapped = map_item(
            "org.example.App",
            &StatusNotifierItem {
                icon_name: None,
                icon_pixmap: Some(vec![IconPixmap {
                    width: 16,
                    height: 16,
                    pixels: vec![255, 1, 2, 3],
                }]),
                ..test_item()
            },
            None,
        );

        assert_eq!(
            mapped.icon,
            Some(TrayIcon::Pixmap {
                width: 16,
                height: 16,
                pixels: vec![255, 1, 2, 3],
            })
        );
    }

    #[test]
    fn map_item_preserves_full_menu_metadata() {
        let menu = TrayMenu {
            id: 1,
            submenus: vec![
                MenuItem {
                    id: 7,
                    menu_type: MenuType::Standard,
                    label: Some("_Enable".into()),
                    enabled: true,
                    visible: true,
                    icon_name: Some("object-select-symbolic".into()),
                    icon_data: None,
                    shortcut: None,
                    toggle_type: ToggleType::Checkmark,
                    toggle_state: ToggleState::On,
                    children_display: None,
                    disposition: Disposition::Informative,
                    submenu: Vec::new(),
                },
                MenuItem {
                    id: 8,
                    menu_type: MenuType::Separator,
                    ..Default::default()
                },
                MenuItem {
                    id: 9,
                    menu_type: MenuType::Standard,
                    label: Some("_More".into()),
                    enabled: true,
                    visible: true,
                    icon_name: None,
                    icon_data: Some(vec![1, 2, 3, 4]),
                    shortcut: Some(vec![vec!["Control".into(), "M".into()]]),
                    toggle_type: ToggleType::Radio,
                    toggle_state: ToggleState::Off,
                    children_display: Some("submenu".into()),
                    disposition: Disposition::Alert,
                    submenu: vec![MenuItem {
                        id: 10,
                        menu_type: MenuType::Standard,
                        label: Some("Child".into()),
                        enabled: false,
                        visible: true,
                        ..Default::default()
                    }],
                },
            ],
        };

        let mapped = map_item("org.example.App", &test_item(), Some(&menu));

        assert_eq!(
            mapped.tooltip,
            Some(TrayTooltip {
                title: "Tooltip title".into(),
                description: "Tooltip body".into(),
                icon: Some(TrayIcon::Name("dialog-information-symbolic".into())),
            })
        );
        assert_eq!(
            mapped.menu,
            vec![
                TrayMenuItem {
                    id: 7,
                    label: "Enable".into(),
                    enabled: true,
                    visible: true,
                    kind: TrayMenuItemKind::Standard,
                    icon: Some(TrayIcon::Name("object-select-symbolic".into())),
                    shortcut: None,
                    toggle_type: TrayMenuToggleType::Checkmark,
                    toggle_state: TrayMenuToggleState::On,
                    children_display: None,
                    disposition: TrayMenuDisposition::Informative,
                    children: Vec::new(),
                },
                TrayMenuItem {
                    id: 8,
                    label: String::new(),
                    enabled: false,
                    visible: true,
                    kind: TrayMenuItemKind::Separator,
                    icon: None,
                    shortcut: None,
                    toggle_type: TrayMenuToggleType::CannotBeToggled,
                    toggle_state: TrayMenuToggleState::Indeterminate,
                    children_display: None,
                    disposition: TrayMenuDisposition::Normal,
                    children: Vec::new(),
                },
                TrayMenuItem {
                    id: 9,
                    label: "More".into(),
                    enabled: true,
                    visible: true,
                    kind: TrayMenuItemKind::Standard,
                    icon: Some(TrayIcon::EncodedBytes(vec![1, 2, 3, 4])),
                    shortcut: Some(vec![vec!["Control".into(), "M".into()]]),
                    toggle_type: TrayMenuToggleType::Radio,
                    toggle_state: TrayMenuToggleState::Off,
                    children_display: Some("submenu".into()),
                    disposition: TrayMenuDisposition::Alert,
                    children: vec![TrayMenuItem {
                        id: 10,
                        label: "Child".into(),
                        enabled: false,
                        visible: true,
                        kind: TrayMenuItemKind::Standard,
                        icon: None,
                        shortcut: None,
                        toggle_type: TrayMenuToggleType::CannotBeToggled,
                        toggle_state: TrayMenuToggleState::Indeterminate,
                        children_display: None,
                        disposition: TrayMenuDisposition::Normal,
                        children: Vec::new(),
                    }],
                },
            ]
        );
    }

    #[test]
    fn snapshot_from_entries_is_stably_sorted() {
        let snapshot = snapshot_from_entries(&[
            ("org.z.Item".into(), (test_item(), None)),
            ("org.a.Item".into(), (test_item(), None)),
        ]);

        assert_eq!(
            snapshot
                .items
                .iter()
                .map(|item| item.address.as_str())
                .collect::<Vec<_>>(),
            vec!["org.a.Item", "org.z.Item"]
        );
    }

    fn test_item() -> StatusNotifierItem {
        StatusNotifierItem {
            id: "example".into(),
            category: Category::ApplicationStatus,
            title: Some("Example".into()),
            status: Status::Active,
            window_id: 0,
            icon_theme_path: None,
            icon_name: Some("example-symbolic".into()),
            icon_pixmap: None,
            overlay_icon_name: None,
            overlay_icon_pixmap: None,
            attention_icon_name: None,
            attention_icon_pixmap: None,
            attention_movie_name: None,
            tool_tip: Some(Tooltip {
                icon_name: "dialog-information-symbolic".into(),
                icon_data: Vec::new(),
                title: "Tooltip title".into(),
                description: "Tooltip body".into(),
            }),
            item_is_menu: false,
            menu: Some("/MenuBar".into()),
        }
    }
}
