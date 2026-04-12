use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    ffi::OsStr,
    process::Stdio,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::process::Command;

use crate::{
    dbus::privacy::{MutterScreenCastSessionProxy, PortalSessionProxy},
    privacy::protocol::{
        PrivacyIndicatorSnapshot, PrivacySession, PrivacySessionAction, PrivacySessionKind,
    },
};

#[async_trait]
pub trait PrivacyBackend: Send {
    async fn snapshot(&mut self) -> anyhow::Result<PrivacyIndicatorSnapshot>;
    async fn stop_all_screen_capture(&mut self) -> anyhow::Result<()>;
    async fn stop_session(&mut self, session_id: &str) -> anyhow::Result<()>;
}

pub struct PrivacyProvider {
    session: zbus::Connection,
    active_screen_captures: HashMap<String, u64>,
    stop_targets: HashMap<String, StopTarget>,
}

impl PrivacyProvider {
    pub fn new(session: zbus::Connection) -> Self {
        Self {
            session,
            active_screen_captures: HashMap::new(),
            stop_targets: HashMap::new(),
        }
    }
}

#[async_trait]
impl PrivacyBackend for PrivacyProvider {
    async fn snapshot(&mut self) -> anyhow::Result<PrivacyIndicatorSnapshot> {
        let (source_outputs, pipewire_dump, camera_device_active) = tokio::join!(
            pactl_json(&["list", "source-outputs"]),
            pw_dump_json(),
            active_camera_devices(),
        );

        let mut mic_sessions = parse_mic_sessions(&source_outputs);
        if source_outputs
            .as_array()
            .is_some_and(|streams| !streams.is_empty())
            && mic_sessions.is_empty()
        {
            mic_sessions.push(placeholder_session(
                "microphone:active",
                "Microphone in use",
                "pulse",
                PrivacySessionKind::Microphone,
            ));
        }

        let pipewire = scan_pipewire_privacy(&pipewire_dump);
        reconcile_screen_captures(
            &mut self.active_screen_captures,
            &pipewire.screen_capture_keys,
            now_unix_secs(),
        );

        let mut stop_targets = HashMap::new();
        let screen_sessions = pipewire
            .screen_sessions
            .into_iter()
            .map(|session| {
                let mut session = session;
                session.started_at = self
                    .active_screen_captures
                    .get(&session.session_id)
                    .copied();
                if let Some(target) = session_stop_target(&session.session_id) {
                    session.stoppable = true;
                    session.supported_action = Some(PrivacySessionAction::StopSession {
                        session_id: session.session_id.clone(),
                    });
                    stop_targets.insert(session.session_id.clone(), target);
                }
                session
            })
            .collect::<Vec<_>>();
        self.stop_targets = stop_targets;

        let mut camera_sessions = pipewire.camera_sessions;
        let mic_active = !mic_sessions.is_empty();
        let camera_active = pipewire.camera_active || camera_device_active;
        let screen_capture_active = !screen_sessions.is_empty();

        if camera_active && camera_sessions.is_empty() {
            camera_sessions.push(placeholder_session(
                "camera:active",
                "Camera in use",
                "pipewire",
                PrivacySessionKind::Camera,
            ));
        }

        let mut sessions = Vec::new();
        sessions.extend(mic_sessions);
        sessions.extend(camera_sessions);
        sessions.extend(screen_sessions.clone());
        sessions.sort_by(|left, right| {
            (
                left.kind.unwrap_or(PrivacySessionKind::Unknown),
                left.app_name.as_str(),
                left.session_id.as_str(),
            )
                .cmp(&(
                    right.kind.unwrap_or(PrivacySessionKind::Unknown),
                    right.app_name.as_str(),
                    right.session_id.as_str(),
                ))
        });

        let mut session_counts = BTreeMap::new();
        for session in &sessions {
            if let Some(kind) = session.kind {
                *session_counts.entry(kind).or_insert(0) += 1;
            }
        }

        Ok(PrivacyIndicatorSnapshot {
            mic_active,
            camera_active,
            screen_capture_active,
            oldest_screen_capture_started_at: screen_sessions
                .iter()
                .filter_map(|session| session.started_at)
                .min(),
            session_counts,
            sessions,
        })
    }

    async fn stop_all_screen_capture(&mut self) -> anyhow::Result<()> {
        stop_all_screen_capture(&self.session, self.stop_targets.values().cloned()).await
    }

    async fn stop_session(&mut self, session_id: &str) -> anyhow::Result<()> {
        if let Some(target) = self.stop_targets.get(session_id).cloned() {
            return stop_target(&self.session, target).await;
        }

        let Some(target) = session_stop_target(session_id) else {
            anyhow::bail!("privacy provider: session {session_id} is not stoppable");
        };
        stop_target(&self.session, target).await
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PipewirePrivacySnapshot {
    camera_active: bool,
    camera_sessions: Vec<PrivacySession>,
    screen_capture_keys: Vec<String>,
    screen_sessions: Vec<PrivacySession>,
}

#[derive(Debug, Clone, Default)]
struct PipewireClientInfo {
    application_name: String,
    application_binary: String,
    pipewire_access: String,
    portal_app_id: String,
}

#[derive(Debug, Clone, Default)]
struct PipewireNodeInfo {
    node_id: u64,
    app_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum StopTarget {
    Portal(String),
    Mutter(String),
}

impl StopTarget {
    fn path(&self) -> &str {
        match self {
            StopTarget::Portal(path) | StopTarget::Mutter(path) => path,
        }
    }
}

async fn pactl_json(args: &[&str]) -> Value {
    let output = Command::new("pactl")
        .args(["--format", "json"])
        .args(args)
        .env("LC_NUMERIC", "C")
        .stderr(Stdio::null())
        .output()
        .await
        .ok();

    output
        .and_then(|output| serde_json::from_slice(&output.stdout).ok())
        .unwrap_or_else(|| json!([]))
}

async fn pw_dump_json() -> Value {
    let output = Command::new("pw-dump")
        .stderr(Stdio::null())
        .output()
        .await
        .ok();

    output
        .and_then(|output| serde_json::from_slice(&output.stdout).ok())
        .unwrap_or_else(|| json!([]))
}

async fn active_camera_devices() -> bool {
    let devices = std::fs::read_dir("/dev")
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with("video"))
        })
        .collect::<Vec<_>>();

    if devices.is_empty() {
        return false;
    }

    let output = Command::new("fuser")
        .args(devices)
        .stderr(Stdio::null())
        .output()
        .await
        .ok();

    output
        .map(|output| parse_fuser_camera_output(&output.stdout))
        .unwrap_or(false)
}

fn parse_mic_sessions(data: &Value) -> Vec<PrivacySession> {
    let Some(streams) = data.as_array() else {
        return Vec::new();
    };

    let mut sessions = Vec::new();
    let mut seen = HashSet::new();
    for (index, stream) in streams.iter().enumerate() {
        let session_id = stream["index"]
            .as_u64()
            .map(|id| format!("microphone:{id}"))
            .unwrap_or_else(|| format!("microphone:stream:{index}"));
        if !seen.insert(session_id.clone()) {
            continue;
        }

        let app_name = first_non_empty_string(&[
            value_pointer(stream, "/properties/application.name"),
            value_pointer(stream, "/proplist/application.name"),
            value_pointer(stream, "/application.name"),
            value_pointer(stream, "/info/props/application.name"),
            value_pointer(stream, "/properties/node.name"),
            value_pointer(stream, "/info/props/node.name"),
        ])
        .unwrap_or("Microphone in use")
        .to_string();

        sessions.push(PrivacySession {
            session_id,
            app_name,
            backend: "pulse".into(),
            started_at: None,
            stoppable: false,
            supported_action: None,
            kind: Some(PrivacySessionKind::Microphone),
        });
    }

    sessions
}

fn parse_fuser_camera_output(stdout: &[u8]) -> bool {
    String::from_utf8_lossy(stdout)
        .chars()
        .any(|ch| ch.is_ascii_digit())
}

fn scan_pipewire_privacy(data: &Value) -> PipewirePrivacySnapshot {
    let Some(objects) = data.as_array() else {
        return PipewirePrivacySnapshot::default();
    };

    let client_map = collect_pipewire_clients(objects);
    let node_map = collect_pipewire_nodes(objects, &client_map);
    let camera_nodes = collect_camera_node_ids(objects);

    let mut snapshot = PipewirePrivacySnapshot {
        camera_sessions: collect_camera_sessions(objects, &camera_nodes, &node_map),
        ..Default::default()
    };
    let mut seen_capture_keys = HashSet::new();

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

        let Some(session) = screen_capture_session(object, props, &client_map) else {
            continue;
        };

        if seen_capture_keys.insert(session.session_id.clone()) {
            snapshot
                .screen_capture_keys
                .push(session.session_id.clone());
            snapshot.screen_sessions.push(session);
        }
    }

    if !snapshot.camera_active {
        snapshot.camera_active =
            !snapshot.camera_sessions.is_empty() || has_active_camera_link(objects, &camera_nodes);
    }

    snapshot
}

fn collect_pipewire_clients(objects: &[Value]) -> HashMap<u64, PipewireClientInfo> {
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
                application_name: json_string(props, "application.name"),
                application_binary: json_string(props, "application.process.binary"),
                pipewire_access: json_string(props, "pipewire.access"),
                portal_app_id: json_string(props, "pipewire.access.portal.app_id"),
            },
        );
    }
    map
}

fn collect_pipewire_nodes(
    objects: &[Value],
    clients: &HashMap<u64, PipewireClientInfo>,
) -> HashMap<u64, PipewireNodeInfo> {
    let mut map = HashMap::new();
    for object in objects {
        if object["type"].as_str() != Some("PipeWire:Interface:Node") {
            continue;
        }

        let Some(node_id) = object["id"].as_u64() else {
            continue;
        };
        let props = &object["info"]["props"];
        let client_id = props["client.id"].as_u64().unwrap_or(0);
        let client = clients.get(&client_id).cloned().unwrap_or_default();
        let app_name = first_non_empty_string(&[
            props["application.name"].as_str(),
            Some(client.application_name.as_str()),
            props["application.process.binary"].as_str(),
            Some(client.application_binary.as_str()),
            props["node.description"].as_str(),
            props["node.name"].as_str(),
        ])
        .unwrap_or("Unknown")
        .to_string();

        map.insert(node_id, PipewireNodeInfo { node_id, app_name });
    }
    map
}

fn collect_camera_node_ids(objects: &[Value]) -> HashSet<u64> {
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

fn collect_camera_sessions(
    objects: &[Value],
    camera_nodes: &HashSet<u64>,
    node_map: &HashMap<u64, PipewireNodeInfo>,
) -> Vec<PrivacySession> {
    let mut sessions = Vec::new();
    let mut seen = HashSet::new();

    for object in objects {
        if object["type"].as_str() != Some("PipeWire:Interface:Link") {
            continue;
        }

        let info = &object["info"];
        if info["state"].as_str() != Some("active") {
            continue;
        }

        let output_node = info["output-node-id"].as_u64().unwrap_or(0);
        let input_node = info["input-node-id"].as_u64().unwrap_or(0);
        let target_node_id = match (
            camera_nodes.contains(&output_node),
            camera_nodes.contains(&input_node),
        ) {
            (true, false) => input_node,
            (false, true) => output_node,
            _ => continue,
        };

        let Some(node) = node_map.get(&target_node_id) else {
            continue;
        };
        let session_id = format!("camera:{}", node.node_id);
        if !seen.insert(session_id.clone()) {
            continue;
        }

        sessions.push(PrivacySession {
            session_id,
            app_name: node.app_name.clone(),
            backend: "pipewire".into(),
            started_at: None,
            stoppable: false,
            supported_action: None,
            kind: Some(PrivacySessionKind::Camera),
        });
    }

    sessions
}

fn is_camera_node(props: &Value) -> bool {
    let media_class = props["media.class"].as_str().unwrap_or("");
    let media_role = props["media.role"].as_str().unwrap_or("");
    let object_path = props["object.path"].as_str().unwrap_or("");
    let node_name = props["node.name"].as_str().unwrap_or("");

    media_class == "Video/Source"
        && (media_role == "Camera"
            || object_path.starts_with("v4l2:")
            || node_name.starts_with("v4l2_"))
}

fn has_active_camera_link(objects: &[Value], camera_nodes: &HashSet<u64>) -> bool {
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

fn screen_capture_session(
    object: &Value,
    props: &Value,
    clients: &HashMap<u64, PipewireClientInfo>,
) -> Option<PrivacySession> {
    let media_class = props["media.class"].as_str().unwrap_or("");
    if !media_class.contains("Video") {
        return None;
    }

    let client_id = props["client.id"].as_u64().unwrap_or(0);
    let client = clients.get(&client_id).cloned().unwrap_or_default();
    let combined = [
        media_class,
        props["media.role"].as_str().unwrap_or(""),
        props["media.name"].as_str().unwrap_or(""),
        props["node.name"].as_str().unwrap_or(""),
        props["node.description"].as_str().unwrap_or(""),
        props["object.path"].as_str().unwrap_or(""),
        props["application.name"].as_str().unwrap_or(""),
        props["application.process.binary"].as_str().unwrap_or(""),
        client.application_name.as_str(),
        client.application_binary.as_str(),
        client.portal_app_id.as_str(),
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

    let app_name = first_non_empty_string(&[
        props["application.name"].as_str(),
        Some(client.application_name.as_str()),
        (!client.portal_app_id.is_empty()).then_some(client.portal_app_id.as_str()),
        props["application.process.binary"].as_str(),
        Some(client.application_binary.as_str()),
        props["node.description"].as_str(),
        props["node.name"].as_str(),
    ])
    .unwrap_or("Screen capture")
    .to_string();
    let session_id = infer_stop_target_from_props(props)
        .map(|target| target.path().to_string())
        .or_else(|| {
            props["object.path"]
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| props["node.name"].as_str().map(ToOwned::to_owned))
                .or_else(|| {
                    object["id"]
                        .as_u64()
                        .map(|id| format!("screen-capture:{id}"))
                })
        })?;

    let kind = if combined.contains("window") {
        PrivacySessionKind::WindowCapture
    } else {
        PrivacySessionKind::ScreenCapture
    };

    Some(PrivacySession {
        session_id,
        app_name,
        backend: "pipewire".into(),
        started_at: None,
        stoppable: false,
        supported_action: None,
        kind: Some(kind),
    })
}

fn infer_stop_target_from_props(props: &Value) -> Option<StopTarget> {
    [
        props["object.path"].as_str(),
        props["target.object"].as_str(),
        props["session.path"].as_str(),
    ]
    .into_iter()
    .flatten()
    .find_map(session_stop_target)
}

fn session_stop_target(session_id: &str) -> Option<StopTarget> {
    if session_id.starts_with("/org/freedesktop/portal/desktop/session/") {
        Some(StopTarget::Portal(session_id.to_string()))
    } else if session_id.starts_with("/org/gnome/Mutter/ScreenCast/Session/") {
        Some(StopTarget::Mutter(session_id.to_string()))
    } else {
        None
    }
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

fn placeholder_session(
    session_id: &str,
    app_name: &str,
    backend: &str,
    kind: PrivacySessionKind,
) -> PrivacySession {
    PrivacySession {
        session_id: session_id.into(),
        app_name: app_name.into(),
        backend: backend.into(),
        started_at: None,
        stoppable: false,
        supported_action: None,
        kind: Some(kind),
    }
}

fn first_non_empty_string<'a>(values: &[Option<&'a str>]) -> Option<&'a str> {
    values
        .iter()
        .copied()
        .flatten()
        .find(|value| !value.is_empty())
}

fn value_pointer<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer)?.as_str()
}

fn json_string(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("").to_string()
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

async fn stop_all_screen_capture(
    session: &zbus::Connection,
    known_targets: impl IntoIterator<Item = StopTarget>,
) -> anyhow::Result<()> {
    let mut stopped_paths = BTreeSet::new();
    let mut last_error = None;

    for target in known_targets {
        match stop_target(session, target.clone()).await {
            Ok(()) => {
                stopped_paths.insert(target.path().to_string());
            }
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    let mut stopped_any = !stopped_paths.is_empty();
    for path in parse_screencast_session_paths(&busctl_tree("org.gnome.Mutter.ScreenCast").await?) {
        if stopped_paths.contains(&path) {
            continue;
        }

        match stop_target(session, StopTarget::Mutter(path.clone())).await {
            Ok(()) => {
                stopped_any = true;
                stopped_paths.insert(path);
            }
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    for path in
        parse_screen_capture_session_paths(&busctl_tree("org.freedesktop.portal.Desktop").await?)
    {
        if stopped_paths.contains(&path) {
            continue;
        }

        match stop_target(session, StopTarget::Portal(path.clone())).await {
            Ok(()) => {
                stopped_any = true;
                stopped_paths.insert(path);
            }
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    if stopped_any {
        Ok(())
    } else {
        Err(anyhow::anyhow!(last_error.unwrap_or_else(|| {
            "no screen capture sessions closed".to_string()
        })))
    }
}

async fn stop_target(session: &zbus::Connection, target: StopTarget) -> anyhow::Result<()> {
    match target {
        StopTarget::Portal(path) => {
            PortalSessionProxy::builder(session)
                .path(path.as_str())?
                .build()
                .await?
                .close()
                .await?;
        }
        StopTarget::Mutter(path) => {
            MutterScreenCastSessionProxy::builder(session)
                .path(path.as_str())?
                .build()
                .await?
                .stop()
                .await?;
        }
    }
    Ok(())
}

async fn busctl_tree(service: &str) -> anyhow::Result<String> {
    let output = Command::new("busctl")
        .args(["--user", "tree", service])
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

#[cfg(test)]
mod tests {
    use super::*;

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
                "/org/freedesktop/portal/desktop/session/1_42/webrtc_session1822154327".to_string(),
                "/org/freedesktop/portal/desktop/session/1_88/screen_cast_session12".to_string(),
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
    fn builds_microphone_sessions_from_source_outputs() {
        let data = json!([
            {
                "index": 41,
                "properties": {
                    "application.name": "Firefox"
                }
            }
        ]);

        assert_eq!(
            parse_mic_sessions(&data),
            vec![PrivacySession {
                session_id: "microphone:41".into(),
                app_name: "Firefox".into(),
                backend: "pulse".into(),
                started_at: None,
                stoppable: false,
                supported_action: None,
                kind: Some(PrivacySessionKind::Microphone),
            }]
        );
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
                "id": 91,
                "type": "PipeWire:Interface:Client",
                "info": {
                    "props": {
                        "application.name": "Zoom",
                        "application.process.binary": "zoom"
                    }
                }
            },
            {
                "id": 120,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "state": "running",
                    "props": {
                        "client.id": 91,
                        "node.name": "zoom-camera-consumer"
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
        assert_eq!(snapshot.camera_sessions.len(), 1);
        assert_eq!(snapshot.camera_sessions[0].app_name, "Zoom");
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
        assert_eq!(
            snapshot.screen_capture_keys,
            vec!["portal:screen:session-1".to_string()]
        );
        assert_eq!(snapshot.screen_sessions.len(), 1);
        assert_eq!(
            snapshot.screen_sessions[0].kind,
            Some(PrivacySessionKind::ScreenCapture)
        );
    }
}
