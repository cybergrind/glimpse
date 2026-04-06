use std::{process::Stdio, rc::Rc, time::Duration};

use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, prelude::*},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
};

use super::{
    config::ExecConfig,
    protocol::{CallbackData, ChildMessage, HeroData, InitData, PanelMessage, StatusItem, TreeNode},
    renderer::{RenderCatalog, apply_icon_to_image},
};

pub struct Exec {
    name: String,
    status: Vec<StatusItem>,
    hero: Option<HeroData>,
    tree: Option<TreeNode>,
    outbound_tx: mpsc::UnboundedSender<PanelMessage>,
    popover: gtk::Popover,
}

#[derive(Clone)]
pub struct ExecInit {
    pub name: String,
    pub config: ExecConfig,
}

#[derive(Debug)]
pub enum ExecMsg {
    ChildMessage(ChildMessage),
    ChildExited,
    Callback(CallbackData),
    TogglePopover,
}

#[relm4::component(pub)]
impl Component for Exec {
    type Init = ExecInit;
    type Input = ExecMsg;
    type Output = ();
    type CommandOutput = ExecMsg;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "exec",
            #[watch]
            set_visible: !model.status.is_empty(),
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = gtk::Popover::new();
        popover.set_parent(&root);
        popover.set_autohide(true);
        popover.add_css_class("exec-popover");

        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let name = init.name.clone();
        let config = init.config.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    run_supervisor(name, config, outbound_rx, out).await;
                })
                .drop_on_shutdown()
        });

        let model = Exec {
            name: init.name,
            status: Vec::new(),
            hero: None,
            tree: None,
            outbound_tx,
            popover,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            ExecMsg::ChildMessage(message) => {
                match message {
                    ChildMessage::Status(data) => self.status = data.items,
                    ChildMessage::Hero(data) => self.hero = data,
                    ChildMessage::Tree(data) => self.tree = data,
                }
                self.rebuild_status(root, &sender);
                self.rebuild_popover(root, &sender);
            }
            ExecMsg::ChildExited => {
                self.status.clear();
                self.hero = None;
                self.tree = None;
                self.popover.popdown();
                self.rebuild_status(root, &sender);
                self.rebuild_popover(root, &sender);
            }
            ExecMsg::Callback(callback) => {
                if let Err(error) = self.outbound_tx.send(PanelMessage::Callback(callback)) {
                    tracing::warn!(%error, applet = %self.name, "exec applet: failed to queue callback");
                }
            }
            ExecMsg::TogglePopover => {
                if self.has_popover_content() {
                    if self.popover.is_visible() {
                        self.popover.popdown();
                    } else {
                        self.popover.popup();
                    }
                }
            }
        }
    }
}

impl Exec {
    fn has_popover_content(&self) -> bool {
        self.hero.is_some() || self.tree.is_some()
    }

    fn rebuild_status(&self, root: &gtk::Box, sender: &ComponentSender<Self>) {
        while let Some(child) = root.first_child() {
            root.remove(&child);
        }

        for (index, item) in self.status.iter().enumerate() {
            root.append(&build_status_item(item, index, self.has_popover_content(), sender));
        }
    }

    fn rebuild_popover(&self, _root: &gtk::Box, sender: &ComponentSender<Self>) {
        if !self.has_popover_content() {
            self.popover.set_child(Option::<&gtk::Widget>::None);
            self.popover.popdown();
            return;
        }

        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);

        if let Some(hero) = &self.hero {
            outer.append(&build_hero(hero));
        }
        if self.hero.is_some() && self.tree.is_some() {
            outer.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        if let Some(tree) = &self.tree {
            let callback_sender = sender.clone();
            let renderer = RenderCatalog::with_callback(Rc::new(move |callback| {
                callback_sender.input(ExecMsg::Callback(callback));
            }));
            match renderer.render(tree) {
                Ok(widget) => outer.append(&widget),
                Err(error) => {
                    tracing::warn!(?error, applet = %self.name, "exec applet: failed to render tree");
                }
            }
        }

        self.popover.set_child(Some(&outer));
    }
}

fn build_status_item(
    item: &StatusItem,
    index: usize,
    has_popover: bool,
    sender: &ComponentSender<Exec>,
) -> gtk::Button {
    let fallback_item = item.clone();
    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.add_css_class("exec-status-item");

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    if let Some(icon) = &item.icon {
        let image = gtk::Image::new();
        apply_icon_to_image(&image, icon);
        image.set_pixel_size(16);
        content.append(&image);
    }
    if let Some(text) = &item.text {
        let label = gtk::Label::new(Some(text));
        label.add_css_class("exec-status-label");
        content.append(&label);
    }
    button.set_child(Some(&content));

    let id = item.id.clone();
    let click_sender = sender.clone();
    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.connect_pressed(move |gesture, _, _, _| {
        if has_popover && gesture.current_button() == 1 {
            click_sender.input(ExecMsg::TogglePopover);
        }
        if let Some(id) = &id {
            click_sender.input(ExecMsg::Callback(CallbackData {
                id: id.clone(),
                event: "click".into(),
                button: Some(mouse_button_name(gesture.current_button()).into()),
                ..CallbackData::default()
            }));
        } else if let Some(message) = PanelMessage::status_click(&fallback_item, index, mouse_button_name(gesture.current_button())) {
            if let PanelMessage::Callback(callback) = message {
                click_sender.input(ExecMsg::Callback(callback));
            }
        }
    });
    button.add_controller(click);

    let scroll_id = item.id.clone();
    let scroll_sender = sender.clone();
    let scroll =
        gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE);
    scroll.connect_scroll(move |_, _, delta_y| {
        if let Some(id) = &scroll_id {
            scroll_sender.input(ExecMsg::Callback(CallbackData {
                id: id.clone(),
                event: "scroll".into(),
                delta_y: Some(delta_y),
                ..CallbackData::default()
            }));
        }
        gtk::glib::Propagation::Proceed
    });
    button.add_controller(scroll);

    button
}

fn build_hero(hero: &HeroData) -> gtk::Box {
    let hero_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hero_box.add_css_class("exec-hero");

    if let Some(icon) = &hero.icon {
        let image = gtk::Image::new();
        image.set_pixel_size(32);
        apply_icon_to_image(&image, icon);
        hero_box.append(&image);
    }

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text_box.set_valign(gtk::Align::Center);

    let title = gtk::Label::new(Some(&hero.title));
    title.set_halign(gtk::Align::Start);
    title.add_css_class("exec-hero-title");
    text_box.append(&title);

    let subtitle = gtk::Label::new(Some(&hero.subtitle));
    subtitle.set_halign(gtk::Align::Start);
    subtitle.add_css_class("exec-hero-subtitle");
    text_box.append(&subtitle);

    hero_box.append(&text_box);
    hero_box
}

fn mouse_button_name(button: u32) -> &'static str {
    match button {
        1 => "left",
        2 => "middle",
        3 => "right",
        _ => "other",
    }
}

async fn run_supervisor(
    name: String,
    config: ExecConfig,
    mut outbound_rx: mpsc::UnboundedReceiver<PanelMessage>,
    out: relm4::Sender<ExecMsg>,
) {
    loop {
        let program = match config.command.first() {
            Some(program) => program.clone(),
            None => {
                tracing::error!(applet = %name, "exec applet: empty command");
                let _ = out.send(ExecMsg::ChildExited);
                return;
            }
        };

        tracing::info!(applet = %name, program = %program, "exec applet: spawning child");
        let mut child = match Command::new(&program)
            .args(config.command.iter().skip(1))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                tracing::warn!(%error, applet = %name, "exec applet: failed to spawn child");
                let _ = out.send(ExecMsg::ChildExited);
                tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
                continue;
            }
        };

        let Some(mut stdin) = child.stdin.take() else {
            tracing::warn!(applet = %name, "exec applet: child missing stdin");
            let _ = out.send(ExecMsg::ChildExited);
            tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
            continue;
        };
        let Some(stdout) = child.stdout.take() else {
            tracing::warn!(applet = %name, "exec applet: child missing stdout");
            let _ = out.send(ExecMsg::ChildExited);
            tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
            continue;
        };

        if let Err(error) = write_message(
            &mut stdin,
            &PanelMessage::Init(InitData {
                instance: name.clone(),
            }),
        )
        .await
        {
            tracing::warn!(%error, applet = %name, "exec applet: failed to send init");
        }

        let mut lines = BufReader::new(stdout).lines();
        let mut should_stop = false;
        loop {
            tokio::select! {
                maybe_message = outbound_rx.recv() => {
                    match maybe_message {
                        Some(message) => {
                            if let Err(error) = write_message(&mut stdin, &message).await {
                                tracing::warn!(%error, applet = %name, "exec applet: failed to write to child");
                                break;
                            }
                        }
                        None => {
                            should_stop = true;
                            let _ = child.kill().await;
                            break;
                        }
                    }
                }
                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) => match serde_json::from_str::<ChildMessage>(&line) {
                            Ok(message) => {
                                let _ = out.send(ExecMsg::ChildMessage(message));
                            }
                            Err(error) => {
                                tracing::warn!(%error, raw = %line, applet = %name, "exec applet: invalid child message");
                            }
                        },
                        Ok(None) => break,
                        Err(error) => {
                            tracing::warn!(%error, applet = %name, "exec applet: stdout read failed");
                            break;
                        }
                    }
                }
                status = child.wait() => {
                    match status {
                        Ok(status) => tracing::info!(?status, applet = %name, "exec applet: child exited"),
                        Err(error) => tracing::warn!(%error, applet = %name, "exec applet: child wait failed"),
                    }
                    break;
                }
            }
        }

        let _ = out.send(ExecMsg::ChildExited);
        if should_stop {
            return;
        }
        tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
    }
}

async fn write_message(
    stdin: &mut tokio::process::ChildStdin,
    message: &PanelMessage,
) -> Result<(), std::io::Error> {
    let encoded = encode_message_line(message);
    stdin.write_all(&encoded).await?;
    stdin.flush().await
}

fn encode_message_line(message: &PanelMessage) -> Vec<u8> {
    let mut encoded =
        serde_json::to_vec(message).expect("exec panel messages should always serialize");
    encoded.push(b'\n');
    encoded
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use relm4::channel;
    use tokio::time::timeout;

    use super::{
        ExecMsg, encode_message_line, mouse_button_name, run_supervisor,
        super::protocol::{CallbackData, InitData, PanelMessage, StatusItem},
    };
    use crate::applets::exec::{
        config::ExecConfig,
        protocol::{ChildMessage, StatusData},
    };

    #[test]
    fn status_item_callbacks_prefer_ids() {
        let item = StatusItem {
            id: Some("wifi".into()),
            icon: None,
            text: Some("Online".into()),
        };

        let callback = PanelMessage::status_click(&item, 0, "left");

        assert_eq!(
            callback,
            Some(PanelMessage::Callback(CallbackData {
                id: "wifi".into(),
                event: "click".into(),
                button: Some("left".into()),
                ..CallbackData::default()
            }))
        );
    }

    #[test]
    fn init_messages_are_encoded_as_json_lines() {
        let encoded = encode_message_line(&PanelMessage::Init(InitData {
            instance: "exec-demo".into(),
        }));

        let text = String::from_utf8(encoded).expect("line should be utf-8");
        assert_eq!(
            text,
            "{\"type\":\"init\",\"data\":{\"instance\":\"exec-demo\"}}\n"
        );
    }

    #[test]
    fn mouse_button_names_match_protocol_values() {
        assert_eq!(mouse_button_name(1), "left");
        assert_eq!(mouse_button_name(2), "middle");
        assert_eq!(mouse_button_name(3), "right");
        assert_eq!(mouse_button_name(9), "other");
    }

    #[tokio::test]
    async fn supervisor_sends_init_to_real_subprocess() {
        let temp_dir = make_temp_dir("exec-init");
        let init_path = temp_dir.join("init.jsonl");
        let script_path = temp_dir.join("child.sh");
        write_script(
            &script_path,
            &format!(
                "#!/usr/bin/env bash\nread line\nprintf '%s\\n' \"$line\" > {}\nprintf '%s\\n' '{{\"type\":\"status\",\"data\":{{\"items\":[{{\"id\":\"demo\",\"text\":\"ready\"}}]}}}}'\nexit 0\n",
                shell_single_quote(&init_path)
            ),
        );

        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        let (sender, receiver) = channel();
        let task = tokio::spawn(run_supervisor(
            "demo".into(),
            ExecConfig {
                command: vec![script_path.to_string_lossy().into_owned()],
                restart_delay_ms: 10_000,
            },
            outbound_rx,
            sender,
        ));

        let message = recv_exec_msg(&receiver).await;
        assert!(matches!(message, ExecMsg::ChildMessage(ChildMessage::Status(StatusData { .. }))));

        let init = fs::read_to_string(&init_path).expect("child should capture init");
        assert_eq!(init, "{\"type\":\"init\",\"data\":{\"instance\":\"demo\"}}\n");

        drop(outbound_tx);
        task.abort();
        let _ = task.await;
    }

    #[tokio::test]
    async fn supervisor_restarts_real_subprocess_after_exit() {
        let temp_dir = make_temp_dir("exec-restart");
        let counter_path = temp_dir.join("counter");
        let script_path = temp_dir.join("child.sh");
        write_script(
            &script_path,
            &format!(
                "#!/usr/bin/env bash\ncount=0\nif [ -f {counter} ]; then count=$(cat {counter}); fi\ncount=$((count + 1))\nprintf '%s' \"$count\" > {counter}\nprintf '%s\\n' \"{{\\\"type\\\":\\\"status\\\",\\\"data\\\":{{\\\"items\\\":[{{\\\"id\\\":\\\"demo\\\",\\\"text\\\":\\\"run$count\\\"}}]}}}}\"\nexit 0\n",
                counter = shell_single_quote(&counter_path)
            ),
        );

        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        let (sender, receiver) = channel();
        let task = tokio::spawn(run_supervisor(
            "demo".into(),
            ExecConfig {
                command: vec![script_path.to_string_lossy().into_owned()],
                restart_delay_ms: 50,
            },
            outbound_rx,
            sender,
        ));

        let first = recv_exec_msg(&receiver).await;
        assert_status_text(&first, "run1");

        let second = recv_exec_msg(&receiver).await;
        assert!(matches!(second, ExecMsg::ChildExited));

        let third = recv_exec_msg(&receiver).await;
        assert_status_text(&third, "run2");

        drop(outbound_tx);
        task.abort();
        let _ = task.await;
    }

    async fn recv_exec_msg(receiver: &relm4::Receiver<ExecMsg>) -> ExecMsg {
        timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("supervisor should emit a message within timeout")
            .expect("receiver should stay open")
    }

    fn assert_status_text(message: &ExecMsg, expected: &str) {
        let ExecMsg::ChildMessage(ChildMessage::Status(StatusData { items })) = message else {
            panic!("expected status message, got {message:?}");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text.as_deref(), Some(expected));
    }

    fn make_temp_dir(prefix: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "glimpse-{prefix}-{}-{unique}",
            nanos
        ));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    fn write_script(path: &Path, script: &str) {
        fs::write(path, script).expect("script should be written");
        let mut permissions = fs::metadata(path)
            .expect("script metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("script should be executable");
    }

    fn shell_single_quote(path: &Path) -> String {
        let raw = path.to_string_lossy();
        format!("'{}'", raw.replace('\'', "'\"'\"'"))
    }
}
