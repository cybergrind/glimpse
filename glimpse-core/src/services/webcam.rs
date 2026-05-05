use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    thread,
};

use pipewire as pw;
use pw::{
    link::{Link, LinkInfoRef},
    node::{Node, NodeInfoRef},
    proxy::{Listener, ProxyT},
    types::ObjectType,
};
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc, watch},
    time::{Duration, sleep},
};
use tokio_util::sync::CancellationToken;

use crate::{
    compositors::{ScreencastKind, ScreencastSession, ScreencastTarget},
    services::framework::{Control, ServiceCommand, ServiceHandle},
};

const COMMAND_QUEUE_SIZE: usize = 4;
const RETRY_DELAY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebcamUsage {
    pub id: String,
    pub app_name: String,
    pub app_icon: String,
    pub camera_name: String,
    pub pipewire_node: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub available: bool,
    pub usages: Vec<WebcamUsage>,
    pub screencasts: Vec<ScreencastSession>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh,
}

pub type WebcamHandle = ServiceHandle<State, Command>;

pub struct WebcamService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

enum MonitorControl {
    Refresh,
    Shutdown,
}

enum MonitorMessage {
    State(State),
    Failed(String),
}

impl WebcamService {
    pub fn new() -> (Self, WebcamHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        loop {
            let outcome = match self.run_inner(cancel.clone()).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    tracing::warn!(%error, "webcam service failed");
                    RunOutcome::RetryAfterDelay
                }
            };

            match outcome {
                RunOutcome::Cancelled => break,
                RunOutcome::RetryAfterDelay => {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = sleep(RETRY_DELAY) => {}
                    }
                }
            }
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<RunOutcome> {
        let (monitor_tx, mut monitor_rx) = mpsc::unbounded_channel();
        let (control_tx, control_rx) = pw::channel::channel();
        let monitor = thread::Builder::new()
            .name("glimpse-webcam-pipewire".into())
            .spawn(move || {
                if let Err(error) = run_pipewire_monitor(monitor_tx.clone(), control_rx) {
                    let _ = monitor_tx.send(MonitorMessage::Failed(error.to_string()));
                }
            })?;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    stop_monitor(control_tx, monitor).await;
                    return Ok(RunOutcome::Cancelled);
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        stop_monitor(control_tx, monitor).await;
                        return Ok(RunOutcome::Cancelled);
                    }
                    Some(ServiceCommand::Control(Control::Start(_) | Control::Reconfigure(_)))
                    | Some(ServiceCommand::Command(Command::Refresh)) => {
                        let _ = control_tx.send(MonitorControl::Refresh);
                    }
                },
                message = monitor_rx.recv() => match message {
                    Some(MonitorMessage::State(state)) => {
                        self.change_state(state);
                    }
                    Some(MonitorMessage::Failed(error)) => {
                        tracing::warn!(%error, "pipewire webcam monitor failed");
                        self.change_state(State::default());
                        stop_monitor(control_tx, monitor).await;
                        return Ok(RunOutcome::RetryAfterDelay);
                    }
                    None => {
                        self.change_state(State::default());
                        stop_monitor(control_tx, monitor).await;
                        return Ok(RunOutcome::RetryAfterDelay);
                    }
                }
            }
        }
    }

    fn change_state(&self, state: State) {
        if *self.state_tx.borrow() == state {
            return;
        }

        if let Err(error) = self.state_tx.send(state) {
            tracing::error!(?error, "failed to publish webcam state");
        }
    }
}

async fn stop_monitor(
    control_tx: pw::channel::Sender<MonitorControl>,
    monitor: thread::JoinHandle<()>,
) {
    let _ = control_tx.send(MonitorControl::Shutdown);
    let _ = tokio::task::spawn_blocking(move || monitor.join()).await;
}

fn run_pipewire_monitor(
    state_tx: mpsc::UnboundedSender<MonitorMessage>,
    control_rx: pw::channel::Receiver<MonitorControl>,
) -> anyhow::Result<()> {
    let main_loop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main_loop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;
    let registry_weak = registry.downgrade();
    let graph = Rc::new(RefCell::new(PipeWireGraph::default()));
    let bound_objects = Rc::new(RefCell::new(BoundObjects::default()));

    let control_loop = main_loop.clone();
    let control_graph = Rc::clone(&graph);
    let control_tx = state_tx.clone();
    let _control = control_rx.attach(main_loop.loop_(), move |command| match command {
        MonitorControl::Refresh => publish_graph(&control_graph, &control_tx),
        MonitorControl::Shutdown => control_loop.quit(),
    });

    let core_loop = main_loop.clone();
    let state_tx_ref = state_tx.clone();
    let _core_listener = core
        .add_listener_local()
        .error(move |id, seq, res, message| {
            tracing::warn!(id, seq, res, message, "pipewire core error");
            if id == pw::core::PW_ID_CORE {
                let _ = state_tx_ref.send(MonitorMessage::Failed(message.to_owned()));
                core_loop.quit();
            }
        })
        .register();

    let graph_ref = Rc::clone(&graph);
    let state_tx_ref = state_tx.clone();
    let bound_objects_ref = Rc::clone(&bound_objects);
    let _registry_listener = registry
        .add_listener_local()
        .global(move |object| {
            let object_id = object.id;

            match object.type_ {
                ObjectType::Client => {
                    graph_ref
                        .borrow_mut()
                        .update_client(object_id, props_from_dict(object.props));
                    publish_graph(&graph_ref, &state_tx_ref);
                }
                ObjectType::Node => {
                    let Some(registry) = registry_weak.upgrade() else {
                        return;
                    };
                    let Ok(node) = registry.bind::<Node, _>(object) else {
                        tracing::debug!(object_id, "failed to bind pipewire node");
                        return;
                    };
                    let graph = Rc::clone(&graph_ref);
                    let tx = state_tx_ref.clone();
                    let listener = node
                        .add_listener_local()
                        .info(move |info| {
                            graph.borrow_mut().update_node(
                                object_id,
                                props_from_dict(info.props()),
                                node_is_running(info),
                            );
                            publish_graph(&graph, &tx);
                        })
                        .register();
                    store_bound_object(&bound_objects_ref, node, Box::new(listener));
                }
                ObjectType::Link => {
                    let Some(registry) = registry_weak.upgrade() else {
                        return;
                    };
                    let Ok(link) = registry.bind::<Link, _>(object) else {
                        tracing::debug!(object_id, "failed to bind pipewire link");
                        return;
                    };
                    let graph = Rc::clone(&graph_ref);
                    let tx = state_tx_ref.clone();
                    let listener = link
                        .add_listener_local()
                        .info(move |info| {
                            graph.borrow_mut().update_link(
                                object_id,
                                info.output_node_id(),
                                info.input_node_id(),
                                link_is_active(info),
                            );
                            publish_graph(&graph, &tx);
                        })
                        .register();
                    store_bound_object(&bound_objects_ref, link, Box::new(listener));
                }
                _ => {}
            }
        })
        .global_remove({
            let graph = Rc::clone(&graph);
            let state_tx = state_tx.clone();
            move |id| {
                graph.borrow_mut().remove_object(id);
                publish_graph(&graph, &state_tx);
            }
        })
        .register();

    publish_graph(&graph, &state_tx);
    main_loop.run();

    Ok(())
}

fn publish_graph(
    graph: &Rc<RefCell<PipeWireGraph>>,
    state_tx: &mpsc::UnboundedSender<MonitorMessage>,
) {
    let _ = state_tx.send(MonitorMessage::State(State {
        available: true,
        usages: graph.borrow().usages(),
        screencasts: graph.borrow().screencasts(),
    }));
}

fn store_bound_object<P: ProxyT + 'static>(
    bound_objects: &Rc<RefCell<BoundObjects>>,
    proxy: P,
    listener: Box<dyn Listener>,
) {
    let proxy_id = proxy.upcast_ref().id();
    let proxy: Box<dyn ProxyT> = Box::new(proxy);
    let bound_objects_weak = Rc::downgrade(bound_objects);
    let proxy_listener = proxy
        .upcast_ref()
        .add_listener_local()
        .removed(move || {
            if let Some(bound_objects) = bound_objects_weak.upgrade() {
                bound_objects.borrow_mut().remove(proxy_id);
            }
        })
        .register();

    bound_objects
        .borrow_mut()
        .insert(proxy_id, proxy, listener, Box::new(proxy_listener));
}

#[derive(Default)]
struct BoundObjects {
    proxies: HashMap<u32, Box<dyn ProxyT>>,
    listeners: HashMap<u32, Vec<Box<dyn Listener>>>,
}

impl BoundObjects {
    fn insert(
        &mut self,
        proxy_id: u32,
        proxy: Box<dyn ProxyT>,
        object_listener: Box<dyn Listener>,
        proxy_listener: Box<dyn Listener>,
    ) {
        self.proxies.insert(proxy_id, proxy);
        self.listeners
            .insert(proxy_id, vec![object_listener, proxy_listener]);
    }

    fn remove(&mut self, proxy_id: u32) {
        self.proxies.remove(&proxy_id);
        self.listeners.remove(&proxy_id);
    }
}

type Props = HashMap<String, String>;

#[derive(Debug, Clone, Default)]
struct PipeWireGraph {
    clients: HashMap<u32, ClientInfo>,
    nodes: HashMap<u32, NodeInfo>,
    links: HashMap<u32, LinkInfo>,
}

impl PipeWireGraph {
    fn update_client(&mut self, id: u32, props: Props) {
        self.clients.insert(id, ClientInfo { props });
    }

    fn update_node(&mut self, id: u32, props: Props, running: bool) {
        self.nodes.insert(id, NodeInfo { props, running });
    }

    fn update_link(&mut self, id: u32, output_node: u32, input_node: u32, active: bool) {
        self.links.insert(
            id,
            LinkInfo {
                output_node,
                input_node,
                active,
            },
        );
    }

    fn remove_object(&mut self, id: u32) {
        self.clients.remove(&id);
        self.nodes.remove(&id);
        self.links.remove(&id);
    }

    fn usages(&self) -> Vec<WebcamUsage> {
        let cameras = self.camera_nodes();
        let mut usages = Vec::new();
        let mut seen = HashSet::new();
        let mut attributed_cameras = HashSet::new();

        for link in self.links.values().filter(|link| link.active) {
            let Some((camera_id, app_id)) =
                camera_link(link.output_node, link.input_node, &cameras)
            else {
                continue;
            };
            let Some(app) = self.node_app_info(app_id) else {
                continue;
            };

            let camera_name = cameras
                .get(&camera_id)
                .cloned()
                .unwrap_or_else(|| "Camera".into());
            let id = format!("webcam:{camera_id}:{app_id}");
            if !seen.insert(id.clone()) {
                continue;
            }

            attributed_cameras.insert(camera_id);
            usages.push(WebcamUsage {
                id,
                app_name: app.app_name,
                app_icon: app.app_icon,
                camera_name,
                pipewire_node: Some(u64::from(app_id)),
            });
        }

        for (camera_id, camera_name) in cameras {
            if attributed_cameras.contains(&camera_id) {
                continue;
            }

            let Some(node) = self.nodes.get(&camera_id) else {
                continue;
            };
            if !node.running {
                continue;
            }

            let id = format!("webcam:{camera_id}:active");
            if !seen.insert(id.clone()) {
                continue;
            }

            usages.push(WebcamUsage {
                id,
                app_name: "Camera in use".into(),
                app_icon: "camera-web-symbolic".into(),
                camera_name,
                pipewire_node: Some(u64::from(camera_id)),
            });
        }

        usages.sort_by(|left, right| {
            (
                left.camera_name.as_str(),
                left.app_name.as_str(),
                left.id.as_str(),
            )
                .cmp(&(
                    right.camera_name.as_str(),
                    right.app_name.as_str(),
                    right.id.as_str(),
                ))
        });
        usages
    }

    fn screencasts(&self) -> Vec<ScreencastSession> {
        let mut screencasts = Vec::new();
        let mut seen = HashSet::new();

        for link in self.links.values().filter(|link| link.active) {
            let Some(output) = self.nodes.get(&link.output_node) else {
                continue;
            };
            let Some(input) = self.nodes.get(&link.input_node) else {
                continue;
            };
            if !is_screencast_output_node(&output.props) || !is_video_consumer_node(&input.props) {
                continue;
            }

            let id = format!(
                "pipewire-screen-capture:{}:{}",
                link.output_node, link.input_node
            );
            if !seen.insert(id.clone()) {
                continue;
            }

            screencasts.push(ScreencastSession {
                id,
                session_id: None,
                kind: ScreencastKind::PipeWire,
                target: ScreencastTarget::Unknown,
                active: true,
                pipewire_node: Some(link.input_node),
                client_pid: None,
                stoppable: false,
            });
        }

        screencasts.sort_by(|left, right| left.id.cmp(&right.id));
        screencasts
    }

    fn camera_nodes(&self) -> HashMap<u32, String> {
        self.nodes
            .iter()
            .filter_map(|(&id, node)| {
                if !is_camera_node(&node.props) {
                    return None;
                }

                let name = first_non_empty(&[
                    prop(&node.props, "node.description"),
                    prop(&node.props, "device.description"),
                    prop(&node.props, "node.name"),
                ])
                .unwrap_or("Camera")
                .to_owned();
                Some((id, name))
            })
            .collect()
    }

    fn node_app_info(&self, id: u32) -> Option<AppInfo> {
        let node = self.nodes.get(&id)?;
        let client = prop(&node.props, "client.id")
            .and_then(|id| id.parse::<u32>().ok())
            .and_then(|id| self.clients.get(&id));

        let app_name = first_non_empty(&[
            prop(&node.props, "application.name"),
            client.and_then(|client| prop(&client.props, "application.name")),
            prop(&node.props, "application.process.binary"),
            client.and_then(|client| prop(&client.props, "application.process.binary")),
            prop(&node.props, "node.description"),
            prop(&node.props, "node.name"),
        ])
        .unwrap_or("Unknown")
        .to_owned();
        let app_icon = first_non_empty(&[
            prop(&node.props, "application.icon_name"),
            client.and_then(|client| prop(&client.props, "application.icon_name")),
        ])
        .unwrap_or("application-x-executable-symbolic")
        .to_owned();

        Some(AppInfo { app_name, app_icon })
    }
}

#[derive(Debug, Clone, Default)]
struct ClientInfo {
    props: Props,
}

#[derive(Debug, Clone, Default)]
struct NodeInfo {
    props: Props,
    running: bool,
}

#[derive(Debug, Clone)]
struct LinkInfo {
    output_node: u32,
    input_node: u32,
    active: bool,
}

struct AppInfo {
    app_name: String,
    app_icon: String,
}

fn props_from_dict(dict: Option<&pw::spa::utils::dict::DictRef>) -> Props {
    dict.map(|dict| {
        dict.iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect()
    })
    .unwrap_or_default()
}

fn node_is_running(info: &NodeInfoRef) -> bool {
    info.as_raw().state == pw::sys::pw_node_state_PW_NODE_STATE_RUNNING
}

fn link_is_active(info: &LinkInfoRef) -> bool {
    info.as_raw().state == pw::sys::pw_link_state_PW_LINK_STATE_ACTIVE
}

fn camera_link(output: u32, input: u32, cameras: &HashMap<u32, String>) -> Option<(u32, u32)> {
    match (cameras.contains_key(&output), cameras.contains_key(&input)) {
        (true, false) => Some((output, input)),
        (false, true) => Some((input, output)),
        _ => None,
    }
}

fn is_camera_node(props: &Props) -> bool {
    let media_class = prop(props, "media.class").unwrap_or("");
    let media_role = prop(props, "media.role").unwrap_or("");
    let object_path = prop(props, "object.path").unwrap_or("");
    let node_name = prop(props, "node.name").unwrap_or("");
    let device_api = prop(props, "device.api").unwrap_or("");
    let factory_name = prop(props, "factory.name").unwrap_or("");

    media_class == "Video/Source"
        && (media_role == "Camera"
            || object_path.starts_with("v4l2:")
            || prop(props, "api.v4l2.path").is_some()
            || node_name.starts_with("v4l2_")
            || node_name.starts_with("v4l2_input")
            || matches!(device_api, "v4l2" | "libcamera")
            || factory_name.contains("v4l2")
            || factory_name.contains("libcamera"))
}

fn is_screencast_output_node(props: &Props) -> bool {
    let media_class = prop(props, "media.class").unwrap_or("");
    if media_class != "Stream/Output/Video" {
        return false;
    }

    let node_name = prop(props, "node.name").unwrap_or("");
    let app_name = prop(props, "application.name").unwrap_or("");
    let binary = prop(props, "application.process.binary").unwrap_or("");
    let text = format!("{node_name} {app_name} {binary}").to_ascii_lowercase();

    text.contains("niri")
        || text.contains("hyprland")
        || text.contains("xdg-desktop-portal")
        || text.contains("screencast")
        || text.contains("screen")
}

fn is_video_consumer_node(props: &Props) -> bool {
    matches!(prop(props, "media.class"), Some("Stream/Input/Video"))
}

fn prop<'a>(props: &'a Props, name: &str) -> Option<&'a str> {
    props
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
}

fn first_non_empty<'a>(items: &[Option<&'a str>]) -> Option<&'a str> {
    items
        .iter()
        .copied()
        .flatten()
        .find(|item| !item.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_webcam_usage_from_active_pipewire_link() {
        let mut graph = PipeWireGraph::default();
        graph.update_node(
            10,
            props(&[
                ("media.class", "Video/Source"),
                ("media.role", "Camera"),
                ("node.description", "Integrated Camera"),
                ("object.path", "v4l2:/dev/video0"),
            ]),
            true,
        );
        graph.update_client(
            20,
            props(&[
                ("application.name", "Firefox"),
                ("application.icon_name", "firefox"),
            ]),
        );
        graph.update_node(
            21,
            props(&[("client.id", "20"), ("node.name", "firefox-camera")]),
            true,
        );
        graph.update_link(30, 10, 21, true);

        assert_eq!(
            graph.usages(),
            vec![WebcamUsage {
                id: "webcam:10:21".into(),
                app_name: "Firefox".into(),
                app_icon: "firefox".into(),
                camera_name: "Integrated Camera".into(),
                pipewire_node: Some(21),
            }]
        );
    }

    #[test]
    fn skips_inactive_pipewire_links() {
        let mut graph = PipeWireGraph::default();
        graph.update_node(
            10,
            props(&[
                ("media.class", "Video/Source"),
                ("media.role", "Camera"),
                ("node.description", "Integrated Camera"),
            ]),
            false,
        );
        graph.update_node(21, props(&[("application.name", "Firefox")]), false);
        graph.update_link(30, 10, 21, false);

        assert!(graph.usages().is_empty());
    }

    #[test]
    fn renders_running_camera_node_without_link() {
        let mut graph = PipeWireGraph::default();
        graph.update_node(
            10,
            props(&[
                ("media.class", "Video/Source"),
                ("media.role", "Camera"),
                ("node.description", "Integrated Camera"),
            ]),
            true,
        );

        assert_eq!(
            graph.usages(),
            vec![WebcamUsage {
                id: "webcam:10:active".into(),
                app_name: "Camera in use".into(),
                app_icon: "camera-web-symbolic".into(),
                camera_name: "Integrated Camera".into(),
                pipewire_node: Some(10),
            }]
        );
    }

    #[test]
    fn identifies_libcamera_video_source_as_camera() {
        assert!(is_camera_node(&props(&[
            ("media.class", "Video/Source"),
            ("device.api", "libcamera"),
            ("node.description", "Integrated Camera"),
        ])));
    }

    #[test]
    fn renders_screencast_usage_from_active_pipewire_video_stream() {
        let mut graph = PipeWireGraph::default();
        graph.update_node(
            10,
            props(&[
                ("media.class", "Stream/Output/Video"),
                ("node.name", "niri"),
            ]),
            true,
        );
        graph.update_node(
            21,
            props(&[
                ("media.class", "Stream/Input/Video"),
                ("node.name", "chrome"),
                ("client.id", "20"),
            ]),
            true,
        );
        graph.update_link(30, 10, 21, true);

        assert_eq!(
            graph.screencasts(),
            vec![ScreencastSession {
                id: "pipewire-screen-capture:10:21".into(),
                session_id: None,
                kind: ScreencastKind::PipeWire,
                target: ScreencastTarget::Unknown,
                active: true,
                pipewire_node: Some(21),
                client_pid: None,
                stoppable: false,
            }]
        );
    }

    #[test]
    fn renders_screencast_usage_from_active_video_link_before_input_runs() {
        let mut graph = PipeWireGraph::default();
        graph.update_node(
            10,
            props(&[
                ("media.class", "Stream/Output/Video"),
                ("node.name", "niri"),
            ]),
            true,
        );
        graph.update_node(
            21,
            props(&[
                ("media.class", "Stream/Input/Video"),
                ("node.name", "chrome"),
            ]),
            false,
        );
        graph.update_link(30, 10, 21, true);

        assert_eq!(graph.screencasts().len(), 1);
    }

    fn props(items: &[(&str, &str)]) -> Props {
        items
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }
}
