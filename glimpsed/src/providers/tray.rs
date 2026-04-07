use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;

use serde::Serialize;
use serde_json::json;
use system_tray::client::{ActivateRequest, Client, Event, UpdateEvent};
use system_tray::data::apply_menu_diffs;
use system_tray::item::{IconPixmap, StatusNotifierItem};
use system_tray::menu::{MenuItem, MenuType, TrayMenu};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "tray";
const TOPICS: &[&str] = &["tray.items"];
const METHODS: &[&str] = &[
    "tray.activate",
    "tray.secondary_activate",
    "tray.activate_menu_item",
];

#[derive(Debug, Clone, Serialize)]
struct TrayItemData {
    address: String,
    title: String,
    icon: String,
    status: String,
    category: String,
    item_is_menu: bool,
    tooltip_title: String,
    menu_path: String,
    menu: Vec<TrayMenuItemData>,
}

#[derive(Debug, Clone, Serialize)]
struct TrayMenuItemData {
    id: i32,
    label: String,
    enabled: bool,
    visible: bool,
    separator: bool,
    children: Vec<TrayMenuItemData>,
}

struct TrayProvider {
    items: HashMap<String, TrayItemData>,
    menus: HashMap<String, TrayMenu>,
    client: Option<Client>,
}

impl Provider for TrayProvider {
    fn name(&self) -> &'static str {
        NAME
    }
    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }
    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }

    fn run(
        &mut self,
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            tracing::info!("tray: starting");
            let client = {
                let mut attempts = 0u32;
                loop {
                    match Client::new().await {
                        Ok(c) => break c,
                        Err(e) => {
                            attempts += 1;
                            if attempts >= 10 {
                                return Err(anyhow::anyhow!(
                                    "failed to create tray client after {attempts} attempts: {e}"
                                ));
                            }
                            let delay = std::time::Duration::from_secs(2u64.pow(attempts.min(4)));
                            tracing::warn!(
                                attempt = attempts,
                                "tray client init failed: {e}, retrying in {delay:?}"
                            );
                            tokio::select! {
                                _ = cancel.cancelled() => return Ok(()),
                                _ = tokio::time::sleep(delay) => {}
                            }
                        }
                    }
                }
            };
            tracing::info!("tray: client connected");
            let mut rx = client.subscribe();
            self.client = Some(client);

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req).await;
                    }
                    event = rx.recv() => {
                        let Ok(event) = event else { break };
                        let changed = self.handle_tray_event(event);
                        if changed {
                            let items: Vec<&TrayItemData> = self.items.values().collect();
                            if events.send(ProviderEvent {
                                topic: "tray.items".into(),
                                data: serde_json::to_value(&items).unwrap_or_default(),
                            }).await.is_err() { break; }
                        }
                    }
                }
            }
            Ok(())
        })
    }
}

impl TrayProvider {
    fn handle_tray_event(&mut self, event: Event) -> bool {
        match event {
            Event::Add(address, item) => {
                tracing::info!(address = %address, title = ?item.title, "tray: item added");
                self.items
                    .insert(address.clone(), item_to_data(&address, &item));
                true
            }
            Event::Update(address, update) => {
                match &update {
                    UpdateEvent::Menu(menu) => {
                        self.menus.insert(address.clone(), menu.clone());
                        if let Some(data) = self.items.get_mut(&address) {
                            data.menu = serialize_menu(&menu.submenus);
                        }
                    }
                    UpdateEvent::MenuDiff(diffs) => {
                        if let Some(menu) = self.menus.get_mut(&address) {
                            apply_menu_diffs(menu, diffs);
                            if let Some(data) = self.items.get_mut(&address) {
                                data.menu = serialize_menu(&menu.submenus);
                            }
                        }
                    }
                    _ => {
                        if let Some(data) = self.items.get_mut(&address) {
                            apply_update(data, update);
                        }
                        return true;
                    }
                }
                true
            }
            Event::Remove(address) => {
                tracing::info!(address = %address, "tray: item removed");
                self.items.remove(&address);
                self.menus.remove(&address);
                true
            }
        }
    }

    async fn handle_request(&self, req: ProviderRequest) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "tray.items" => {
                        let items: Vec<&TrayItemData> = self.items.values().collect();
                        serde_json::to_value(&items).ok()
                    }
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call {
                method,
                params,
                reply,
            } => {
                let result = match method.as_str() {
                    "tray.activate" => {
                        tracing::info!(
                            "activating tray item {}",
                            params["address"].as_str().unwrap_or("?")
                        );
                        self.activate(&method, &params).await
                    }
                    "tray.secondary_activate" => {
                        tracing::info!(
                            "secondary-activating tray item {}",
                            params["address"].as_str().unwrap_or("?")
                        );
                        self.activate(&method, &params).await
                    }
                    "tray.activate_menu_item" => {
                        tracing::info!(
                            "activating menu item {} on {}",
                            params["submenu_id"],
                            params["address"].as_str().unwrap_or("?")
                        );
                        self.activate(&method, &params).await
                    }
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                let _ = reply.send(result);
            }
        }
    }

    async fn activate(
        &self,
        method: &str,
        params: &serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let Some(client) = &self.client else {
            anyhow::bail!("tray client not initialized");
        };
        let address = params["address"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'address'"))?
            .to_owned();
        let x = params["x"].as_i64().unwrap_or(0) as i32;
        let y = params["y"].as_i64().unwrap_or(0) as i32;

        let request = match method {
            "tray.activate" => ActivateRequest::Default { address, x, y },
            "tray.secondary_activate" => ActivateRequest::Secondary { address, x, y },
            "tray.activate_menu_item" => {
                let menu_path = params["menu_path"].as_str().unwrap_or("").to_owned();
                let submenu_id = params["submenu_id"].as_i64().unwrap_or(0) as i32;
                ActivateRequest::MenuItem {
                    address,
                    menu_path,
                    submenu_id,
                }
            }
            _ => anyhow::bail!("unknown method"),
        };

        client
            .activate(request)
            .await
            .map_err(|e| anyhow::anyhow!("activate failed: {e}"))?;
        Ok(json!(null))
    }
}

fn item_to_data(address: &str, item: &StatusNotifierItem) -> TrayItemData {
    let icon = resolve_icon(
        address,
        item.icon_name.as_deref(),
        item.icon_pixmap.as_deref(),
    );

    let tooltip_title = item
        .tool_tip
        .as_ref()
        .map(|t| t.title.clone())
        .unwrap_or_default();

    TrayItemData {
        address: address.to_owned(),
        title: item.title.clone().unwrap_or_default(),
        icon,
        status: format!("{:?}", item.status),
        category: format!("{:?}", item.category),
        item_is_menu: item.item_is_menu,
        tooltip_title,
        menu_path: item.menu.clone().unwrap_or_default(),
        menu: Vec::new(),
    }
}

fn apply_update(data: &mut TrayItemData, update: UpdateEvent) {
    match update {
        UpdateEvent::Icon {
            icon_name,
            icon_pixmap,
        } => {
            data.icon = resolve_icon(&data.address, icon_name.as_deref(), icon_pixmap.as_deref());
        }
        UpdateEvent::Title(title) => {
            if let Some(t) = title {
                data.title = t;
            }
        }
        UpdateEvent::Status(status) => {
            data.status = format!("{status:?}");
        }
        UpdateEvent::MenuConnect(path) => {
            data.menu_path = path;
        }
        _ => {}
    }
}

/// Resolve an icon to a name or file path. If only pixmap data is available,
/// write it to a temp PNG and return the path.
fn resolve_icon(
    address: &str,
    icon_name: Option<&str>,
    icon_pixmap: Option<&[IconPixmap]>,
) -> String {
    // Prefer icon name or path.
    if let Some(name) = icon_name.filter(|n| !n.is_empty()) {
        return name.to_owned();
    }

    // Write pixmap to temp file.
    if let Some(pixmap) = icon_pixmap.and_then(|ps| ps.iter().max_by_key(|p| p.width)) {
        if let Some(path) = write_pixmap_to_file(address, pixmap) {
            return path;
        }
    }

    "image-missing-symbolic".to_owned()
}

/// Write ARGB pixmap as a PNG temp file. Returns the file path.
fn write_pixmap_to_file(address: &str, pixmap: &IconPixmap) -> Option<String> {
    let dir = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let icon_dir = PathBuf::from(&dir).join("glimpsed-icons");
    std::fs::create_dir_all(&icon_dir).ok()?;

    let safe_name: String = address
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = icon_dir.join(format!("{safe_name}.png"));

    // Convert ARGB to RGBA.
    let argb = &pixmap.pixels;
    let mut rgba = vec![0u8; argb.len()];
    for i in (0..argb.len()).step_by(4) {
        if i + 3 < argb.len() {
            rgba[i] = argb[i + 1]; // R
            rgba[i + 1] = argb[i + 2]; // G
            rgba[i + 2] = argb[i + 3]; // B
            rgba[i + 3] = argb[i]; // A
        }
    }

    let file = std::fs::File::create(&path).ok()?;
    let mut encoder = png::Encoder::new(file, pixmap.width as u32, pixmap.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&rgba).ok()?;

    Some(path.to_string_lossy().into_owned())
}

fn serialize_menu(items: &[MenuItem]) -> Vec<TrayMenuItemData> {
    items
        .iter()
        .filter(|i| i.visible)
        .map(|item| TrayMenuItemData {
            id: item.id,
            label: item
                .label
                .as_deref()
                .unwrap_or("")
                .replace("__", "\x00")
                .replace('_', "")
                .replace('\x00', "_"),
            enabled: item.enabled,
            visible: item.visible,
            separator: item.menu_type == MenuType::Separator,
            children: serialize_menu(&item.submenu),
        })
        .collect()
}

pub struct TrayProviderFactory;

impl ProviderFactory for TrayProviderFactory {
    fn name(&self) -> &'static str {
        NAME
    }
    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }
    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }
    fn create(&self) -> Box<dyn Provider> {
        Box::new(TrayProvider {
            items: HashMap::new(),
            menus: HashMap::new(),
            client: None,
        })
    }
}
