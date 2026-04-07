use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::json;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "privacy";
const TOPICS: &[&str] = &["privacy.indicators"];
const METHODS: &[&str] = &["privacy.stop_screen_capture"];

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
struct PrivacyIndicators {
    mic_active: bool,
    camera_active: bool,
    screen_capture_active: bool,
    screen_capture_started_at: Option<u64>,
    screen_capture_count: u32,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PipewirePrivacySnapshot {
    camera_active: bool,
    screen_capture_keys: Vec<String>,
}

struct PrivacyProvider {
    indicators: PrivacyIndicators,
    active_screen_captures: HashMap<String, u64>,
}

impl Provider for PrivacyProvider {
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
            tracing::info!("privacy: starting");
            self.refresh().await;
            self.emit(&events).await;

            let mut interval = tokio::time::interval(Duration::from_secs(1));

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req).await;
                    }
                    _ = interval.tick() => {
                        let previous = self.indicators.clone();
                        self.refresh().await;
                        if self.indicators != previous {
                            tracing::info!(
                                mic_active = self.indicators.mic_active,
                                camera_active = self.indicators.camera_active,
                                screen_capture_active = self.indicators.screen_capture_active,
                                screen_capture_count = self.indicators.screen_capture_count,
                                "privacy: indicators changed"
                            );
                            self.emit(&events).await;
                        }
                    }
                }
            }

            tracing::info!("privacy: stopping");
            Ok(())
        })
    }
}

impl PrivacyProvider {
    async fn refresh(&mut self) {
        let (source_outputs, pipewire_dump, camera_device_active) = tokio::join!(
            pactl_json(&["list", "source-outputs"]),
            pw_dump_json(),
            fuser_camera_active()
        );
        let pw = scan_pipewire_privacy(&pipewire_dump);

        self.indicators.mic_active = parse_mic_active(&source_outputs);
        self.indicators.camera_active = pw.camera_active || camera_device_active;

        reconcile_screen_captures(
            &mut self.active_screen_captures,
            &pw.screen_capture_keys,
            now_unix_secs(),
        );

        self.indicators.screen_capture_active = !self.active_screen_captures.is_empty();
        self.indicators.screen_capture_count = self.active_screen_captures.len() as u32;
        self.indicators.screen_capture_started_at =
            self.active_screen_captures.values().min().copied();
    }

    async fn emit(&self, events: &mpsc::Sender<ProviderEvent>) {
        let _ = events
            .send(ProviderEvent {
                topic: "privacy.indicators".into(),
                data: serde_json::to_value(&self.indicators).unwrap_or_default(),
            })
            .await;
    }

    async fn handle_request(&self, req: ProviderRequest) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "privacy.indicators" => serde_json::to_value(&self.indicators).ok(),
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call { method, reply, .. } => {
                let result = match method.as_str() {
                    "privacy.stop_screen_capture" => stop_screen_capture_sessions().await,
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                let _ = reply.send(result);
            }
        }
    }
}

async fn pactl_json(args: &[&str]) -> serde_json::Value {
    let output = Command::new("pactl")
        .args(["--format", "json"])
        .args(args)
        .env("LC_NUMERIC", "C")
        .stderr(Stdio::null())
        .output()
        .await
        .ok();
    output
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or(json!([]))
}

async fn pw_dump_json() -> serde_json::Value {
    let output = Command::new("pw-dump")
        .stderr(Stdio::null())
        .output()
        .await
        .ok();
    output
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or(json!([]))
}

async fn fuser_camera_active() -> bool {
    let output = Command::new("fuser")
        .args(["/dev/video0", "/dev/video1", "/dev/video2", "/dev/video3"])
        .stderr(Stdio::null())
        .output()
        .await
        .ok();

    output
        .map(|output| parse_fuser_camera_output(&output.stdout))
        .unwrap_or(false)
}

fn parse_mic_active(data: &serde_json::Value) -> bool {
    data.as_array().is_some_and(|streams| !streams.is_empty())
}

fn parse_fuser_camera_output(stdout: &[u8]) -> bool {
    String::from_utf8_lossy(stdout)
        .chars()
        .any(|ch| ch.is_ascii_digit())
}

fn scan_pipewire_privacy(data: &serde_json::Value) -> PipewirePrivacySnapshot {
    let Some(objects) = data.as_array() else {
        return PipewirePrivacySnapshot::default();
    };

    let client_map = collect_pipewire_clients(objects);
    let camera_nodes = collect_camera_node_ids(objects);
    let mut snapshot = PipewirePrivacySnapshot::default();
    let mut seen = HashSet::new();

    for object in objects {
        if object["type"].as_str() != Some("PipeWire:Interface:Node") {
            continue;
        }

        let info = &object["info"];
        if info["state"].as_str() != Some("running") {
            continue;
        }

        let props = &info["props"];
        if is_camera_node(props) {
            snapshot.camera_active = true;
            continue;
        }

        if let Some(key) = screen_capture_key(object, props, &client_map) {
            if seen.insert(key.clone()) {
                snapshot.screen_capture_keys.push(key);
            }
        }
    }

    if !snapshot.camera_active {
        snapshot.camera_active = has_active_camera_link(objects, &camera_nodes);
    }

    snapshot
}

#[derive(Debug, Clone, Default)]
struct PipewireClientInfo {
    application_name: String,
    application_binary: String,
    pipewire_access: String,
    portal_app_id: String,
}

fn collect_pipewire_clients(objects: &[serde_json::Value]) -> HashMap<u64, PipewireClientInfo> {
    let mut map = HashMap::new();

    for object in objects {
        if object["type"].as_str() != Some("PipeWire:Interface:Client") {
            continue;
        }

        let Some(client_id) = object["id"].as_u64() else {
            continue;
        };

        let props = &object["info"]["props"];
        map.insert(
            client_id,
            PipewireClientInfo {
                application_name: props["application.name"].as_str().unwrap_or("").to_owned(),
                application_binary: props["application.process.binary"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned(),
                pipewire_access: props["pipewire.access"].as_str().unwrap_or("").to_owned(),
                portal_app_id: props["pipewire.access.portal.app_id"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned(),
            },
        );
    }

    map
}

fn is_camera_node(props: &serde_json::Value) -> bool {
    let media_class = props["media.class"].as_str().unwrap_or("");
    let media_role = props["media.role"].as_str().unwrap_or("");
    let object_path = props["object.path"].as_str().unwrap_or("");
    let node_name = props["node.name"].as_str().unwrap_or("");

    media_class == "Video/Source"
        && (media_role == "Camera"
            || object_path.starts_with("v4l2:")
            || node_name.starts_with("v4l2_"))
}

fn collect_camera_node_ids(objects: &[serde_json::Value]) -> HashSet<u64> {
    objects
        .iter()
        .filter(|object| object["type"].as_str() == Some("PipeWire:Interface:Node"))
        .filter_map(|object| {
            let props = &object["info"]["props"];
            if is_camera_node(props) {
                object["id"].as_u64()
            } else {
                None
            }
        })
        .collect()
}

fn has_active_camera_link(objects: &[serde_json::Value], camera_nodes: &HashSet<u64>) -> bool {
    if camera_nodes.is_empty() {
        return false;
    }

    objects.iter().any(|object| {
        if object["type"].as_str() != Some("PipeWire:Interface:Link") {
            return false;
        }

        let info = &object["info"];
        if info["state"].as_str() != Some("active") {
            return false;
        }

        let output_node = info["output-node-id"].as_u64().unwrap_or(0);
        let input_node = info["input-node-id"].as_u64().unwrap_or(0);
        camera_nodes.contains(&output_node) || camera_nodes.contains(&input_node)
    })
}

fn screen_capture_key(
    object: &serde_json::Value,
    props: &serde_json::Value,
    client_map: &HashMap<u64, PipewireClientInfo>,
) -> Option<String> {
    let media_class = props["media.class"].as_str().unwrap_or("");
    if !media_class.contains("Video") {
        return None;
    }

    let client_id = props["client.id"].as_u64().unwrap_or(0);
    let client = client_map
        .get(&client_id)
        .cloned()
        .unwrap_or_default();

    let combined = [
        media_class,
        props["media.role"].as_str().unwrap_or(""),
        props["media.name"].as_str().unwrap_or(""),
        props["node.name"].as_str().unwrap_or(""),
        props["node.description"].as_str().unwrap_or(""),
        props["object.path"].as_str().unwrap_or(""),
        props["application.name"].as_str().unwrap_or(""),
        props["application.process.binary"].as_str().unwrap_or(""),
        &client.application_name,
        &client.application_binary,
        &client.portal_app_id,
    ]
    .join(" ")
    .to_lowercase();

    let portal_owned = client.application_binary.starts_with("xdg-desktop-portal")
        || client.application_binary == "xdpw"
        || client.application_name.to_lowercase().contains("portal")
        || client.pipewire_access == "portal"
        || !client.portal_app_id.is_empty();
    let has_screen_marker = combined.contains("screen")
        || combined.contains("screencast")
        || combined.contains("webrtc")
        || combined.contains("portal")
        || combined.contains("xdpw")
        || combined.contains("monitor")
        || combined.contains("window");

    if !portal_owned && !has_screen_marker {
        return None;
    }

    props["object.path"]
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| props["node.name"].as_str().map(ToOwned::to_owned))
        .or_else(|| object["id"].as_u64().map(|id| format!("node:{id}")))
}

fn reconcile_screen_captures(
    active: &mut HashMap<String, u64>,
    current_keys: &[String],
    now_secs: u64,
) {
    let current: HashSet<&str> = current_keys.iter().map(String::as_str).collect();
    active.retain(|key, _| current.contains(key.as_str()));

    for key in current_keys {
        active.entry(key.clone()).or_insert(now_secs);
    }
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

async fn stop_screen_capture_sessions() -> anyhow::Result<serde_json::Value> {
    let conn = zbus::Connection::session().await?;
    let mut closed = stop_screencast_sessions(&conn).await?;
    let mut last_error = None;

    if closed == 0 {
        let tree = portal_session_tree().await?;
        let paths = parse_screen_capture_session_paths(&tree);

        for path in paths {
            match conn
                .call_method(
                    Some("org.freedesktop.portal.Desktop"),
                    path.as_str(),
                    Some("org.freedesktop.portal.Session"),
                    "Close",
                    &(),
                )
                .await
            {
                Ok(_) => closed += 1,
                Err(error) => last_error = Some(error.to_string()),
            }
        }
    }

    if closed == 0 {
        return Err(anyhow::anyhow!(
            last_error.unwrap_or_else(|| "no screen capture sessions closed".to_string())
        ));
    }

    Ok(json!({ "closed": closed }))
}

async fn stop_screencast_sessions(conn: &zbus::Connection) -> anyhow::Result<u32> {
    let tree = screencast_session_tree().await?;
    let paths = parse_screencast_session_paths(&tree);
    let mut closed = 0_u32;

    for path in paths {
        conn.call_method(
            Some("org.gnome.Mutter.ScreenCast"),
            path.as_str(),
            Some("org.gnome.Mutter.ScreenCast.Session"),
            "Stop",
            &(),
        )
        .await?;
        closed += 1;
    }

    Ok(closed)
}

async fn screencast_session_tree() -> anyhow::Result<String> {
    let output = Command::new("busctl")
        .args(["--user", "tree", "org.gnome.Mutter.ScreenCast"])
        .stderr(Stdio::null())
        .output()
        .await?;

    if !output.status.success() {
        return Ok(String::new());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn portal_session_tree() -> anyhow::Result<String> {
    let output = Command::new("busctl")
        .args(["--user", "tree", "org.freedesktop.portal.Desktop"])
        .stderr(Stdio::null())
        .output()
        .await?;

    if !output.status.success() {
        return Ok(String::new());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_screen_capture_session_paths(tree: &str) -> Vec<String> {
    let mut sessions = Vec::new();

    for line in tree.lines() {
        let line = line.trim();
        if !line.starts_with('└') && !line.starts_with('├') {
            continue;
        }
        let Some(path) = line.split_whitespace().last() else {
            continue;
        };
        if !path.starts_with("/org/freedesktop/portal/desktop/session/") {
            continue;
        }
        let lower = path.to_lowercase();
        if lower.contains("webrtc_session")
            || lower.contains("screen")
            || lower.contains("screencast")
            || lower.contains("cast")
        {
            sessions.push(path.to_owned());
        }
    }

    sessions.sort();
    sessions.dedup();
    sessions
}

fn parse_screencast_session_paths(tree: &str) -> Vec<String> {
    let mut sessions = Vec::new();

    for line in tree.lines() {
        let line = line.trim();
        let Some(path) = line.split_whitespace().last() else {
            continue;
        };
        if path.starts_with("/org/gnome/Mutter/ScreenCast/Session/")
            && path != "/org/gnome/Mutter/ScreenCast/Session"
        {
            sessions.push(path.to_owned());
        }
    }

    sessions.sort();
    sessions.dedup();
    sessions
}

pub struct PrivacyProviderFactory;

impl ProviderFactory for PrivacyProviderFactory {
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
        Box::new(PrivacyProvider {
            indicators: PrivacyIndicators::default(),
            active_screen_captures: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_metadata_is_exposed() {
        assert_eq!(NAME, "privacy");
        assert_eq!(TOPICS, &["privacy.indicators"]);
        assert_eq!(METHODS, &["privacy.stop_screen_capture"]);
    }

    #[test]
    fn mic_activity_uses_source_outputs() {
        assert!(!parse_mic_active(&json!([])));
        assert!(parse_mic_active(&json!([{ "index": 41 }])));
    }

    #[test]
    fn parses_camera_device_usage_from_fuser_output() {
        assert!(parse_fuser_camera_output(b" 137730"));
        assert!(!parse_fuser_camera_output(b""));
        assert!(!parse_fuser_camera_output(b"   \n"));
    }

    #[test]
    fn parses_screen_capture_portal_sessions_from_tree() {
        let tree = r#"
└─ /org
  └─ /org/freedesktop
    └─ /org/freedesktop/portal
      └─ /org/freedesktop/portal/desktop
        └─ /org/freedesktop/portal/desktop/session
          ├─ /org/freedesktop/portal/desktop/session/1_42/webrtc_session1822154327
          ├─ /org/freedesktop/portal/desktop/session/1_4269/gtk1394200140
          └─ /org/freedesktop/portal/desktop/session/1_88/screen_cast_session12
"#;

        assert_eq!(
            parse_screen_capture_session_paths(tree),
            vec![
                "/org/freedesktop/portal/desktop/session/1_42/webrtc_session1822154327"
                    .to_string(),
                "/org/freedesktop/portal/desktop/session/1_88/screen_cast_session12"
                    .to_string(),
            ]
        );
    }

    #[test]
    fn parses_screencast_session_paths_from_mutter_tree() {
        let tree = r#"
└─ /org
  └─ /org/gnome
    └─ /org/gnome/Mutter
      └─ /org/gnome/Mutter/ScreenCast
        ├─ /org/gnome/Mutter/ScreenCast/Session
        │ ├─ /org/gnome/Mutter/ScreenCast/Session/u5
        │ └─ /org/gnome/Mutter/ScreenCast/Session/u7
        └─ /org/gnome/Mutter/ScreenCast/Stream
          └─ /org/gnome/Mutter/ScreenCast/Stream/u5
"#;

        assert_eq!(
            parse_screencast_session_paths(tree),
            vec![
                "/org/gnome/Mutter/ScreenCast/Session/u5".to_string(),
                "/org/gnome/Mutter/ScreenCast/Session/u7".to_string(),
            ]
        );
    }

    #[test]
    fn detects_running_camera_nodes() {
        let data = json!([
            {
                "id": 66,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "state": "running",
                    "props": {
                        "media.class": "Video/Source",
                        "media.role": "Camera",
                        "object.path": "v4l2:/dev/video0",
                        "node.name": "v4l2_input.usb-camera"
                    }
                }
            }
        ]);

        let snapshot = scan_pipewire_privacy(&data);
        assert!(snapshot.camera_active);
        assert!(snapshot.screen_capture_keys.is_empty());
    }

    #[test]
    fn detects_camera_activity_from_active_links() {
        let data = json!([
            {
                "id": 66,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "state": "suspended",
                    "props": {
                        "media.class": "Video/Source",
                        "media.role": "Camera",
                        "object.path": "v4l2:/dev/video0",
                        "node.name": "v4l2_input.usb-camera"
                    }
                }
            },
            {
                "id": 90,
                "type": "PipeWire:Interface:Link",
                "info": {
                    "state": "active",
                    "output-node-id": 66,
                    "input-node-id": 120
                }
            }
        ]);

        let snapshot = scan_pipewire_privacy(&data);
        assert!(snapshot.camera_active);
    }

    #[test]
    fn detects_portal_owned_screen_capture_nodes() {
        let data = json!([
            {
                "id": 32,
                "type": "PipeWire:Interface:Client",
                "info": {
                    "props": {
                        "application.name": "xdg-desktop-portal",
                        "application.process.binary": "xdg-desktop-portal"
                    }
                }
            },
            {
                "id": 87,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "state": "running",
                    "props": {
                        "client.id": 32,
                        "media.class": "Stream/Input/Video",
                        "node.name": "xdpw-screen-cast",
                        "object.path": "portal:screen:session-1"
                    }
                }
            }
        ]);

        let snapshot = scan_pipewire_privacy(&data);
        assert!(!snapshot.camera_active);
        assert_eq!(
            snapshot.screen_capture_keys,
            vec!["portal:screen:session-1".to_string()]
        );
    }

    #[test]
    fn detects_portal_video_consumer_streams() {
        let data = json!([
            {
                "id": 91,
                "type": "PipeWire:Interface:Client",
                "info": {
                    "props": {
                        "application.name": "chrome",
                        "application.process.binary": "chrome",
                        "pipewire.access": "portal",
                        "pipewire.access.portal.app_id": "com.google.Chrome"
                    }
                }
            },
            {
                "id": 104,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "state": "running",
                    "props": {
                        "client.id": 91,
                        "media.class": "Stream/Input/Video",
                        "media.name": "webrtc-consume-stream",
                        "node.name": "chrome"
                    }
                }
            }
        ]);

        let snapshot = scan_pipewire_privacy(&data);
        assert_eq!(snapshot.screen_capture_keys, vec!["chrome".to_string()]);
    }

    #[test]
    fn detects_compositor_screen_cast_sources() {
        let data = json!([
            {
                "id": 111,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "state": "running",
                    "props": {
                        "client.id": 103,
                        "media.class": "Stream/Output/Video",
                        "media.name": "niri-screen-cast-src",
                        "node.name": "niri"
                    }
                }
            }
        ]);

        let snapshot = scan_pipewire_privacy(&data);
        assert_eq!(snapshot.screen_capture_keys, vec!["niri".to_string()]);
    }

    #[test]
    fn ignores_non_portal_video_streams_without_screen_markers() {
        let data = json!([
            {
                "id": 52,
                "type": "PipeWire:Interface:Client",
                "info": {
                    "props": {
                        "application.name": "OBS",
                        "application.process.binary": "obs"
                    }
                }
            },
            {
                "id": 88,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "state": "running",
                    "props": {
                        "client.id": 52,
                        "media.class": "Stream/Input/Video",
                        "node.name": "obs-preview",
                        "object.path": "obs:preview"
                    }
                }
            }
        ]);

        let snapshot = scan_pipewire_privacy(&data);
        assert!(snapshot.screen_capture_keys.is_empty());
    }

    #[test]
    fn preserves_existing_screen_capture_start_time() {
        let mut active = HashMap::from([
            ("portal:screen:old".to_string(), 100_u64),
            ("portal:screen:gone".to_string(), 120_u64),
        ]);

        reconcile_screen_captures(
            &mut active,
            &[
                "portal:screen:old".to_string(),
                "portal:screen:new".to_string(),
            ],
            150,
        );

        assert_eq!(active.get("portal:screen:old"), Some(&100));
        assert_eq!(active.get("portal:screen:new"), Some(&150));
        assert!(!active.contains_key("portal:screen:gone"));
    }
}
