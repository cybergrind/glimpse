use std::{process::Stdio, time::Duration};

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
};

use super::{
    config::ExecConfig,
    popover::{ExecPopover, ExecPopoverInit, ExecPopoverInput, ExecPopoverOutput},
    protocol::{
        CallbackData, ChildMessage, HeroData, InitData, PanelMessage, StatusItem, TreeNode,
    },
    renderer::apply_icon_to_image,
};

pub struct Exec {
    name: String,
    status: Vec<StatusItem>,
    hero: Option<HeroData>,
    tree: Option<TreeNode>,
    outbound_tx: mpsc::UnboundedSender<PanelMessage>,
    restart_tx: mpsc::UnboundedSender<SupervisorControl>,
    popover: Controller<ExecPopover>,
    trigger: gtk::MenuButton,
    status_box: gtk::Box,
    context_menu: gtk::PopoverMenu,
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
    RestartCommand,
    TogglePopover,
}

#[derive(Debug)]
enum SupervisorControl {
    Restart,
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
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = ExecPopover::builder()
            .launch(ExecPopoverInit {
                applet_name: init.name.clone(),
            })
            .forward(sender.input_sender(), |msg| match msg {
                ExecPopoverOutput::Callback(cb) => ExecMsg::Callback(cb),
            });
        let trigger = gtk::MenuButton::new();
        trigger.set_has_frame(false);
        trigger.add_css_class("flat");
        trigger.add_css_class("exec-trigger");
        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        trigger.set_child(Some(&status_box));
        trigger.set_popover(Some(popover.widget()));
        root.append(&trigger);
        root.set_visible(false);

        let action_group = gtk::gio::SimpleActionGroup::new();
        let restart_action = gtk::gio::SimpleAction::new("restart_command", None);
        restart_action.connect_activate({
            let sender = sender.input_sender().clone();
            move |_, _| sender.emit(ExecMsg::RestartCommand)
        });
        action_group.add_action(&restart_action);
        root.insert_action_group("exec", Some(&action_group));

        let context_menu = gtk::PopoverMenu::from_model(Some(&{
            let menu = gtk::gio::Menu::new();
            menu.append(Some("Restart command"), Some("exec.restart_command"));
            menu
        }));
        context_menu.set_parent(&root);
        context_menu.set_has_arrow(false);
        {
            let context_menu = context_menu.clone();
            root.connect_destroy(move |_| context_menu.unparent());
        }

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        let context_menu_ref = context_menu.clone();
        right_click.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            context_menu_ref.popup();
        });
        root.add_controller(right_click);

        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (restart_tx, restart_rx) = mpsc::unbounded_channel();
        let name = init.name.clone();
        let config = init.config.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    run_supervisor(name, config, outbound_rx, restart_rx, out).await;
                })
                .drop_on_shutdown()
        });

        let model = Exec {
            name: init.name,
            status: Vec::new(),
            hero: None,
            tree: None,
            outbound_tx,
            restart_tx,
            popover,
            trigger,
            status_box,
            context_menu,
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
                    ChildMessage::Hero(data) => {
                        self.hero = data.clone();
                        self.popover.emit(ExecPopoverInput::SetHero(data));
                    }
                    ChildMessage::Tree(data) => {
                        self.tree = data.clone();
                        self.popover.emit(ExecPopoverInput::SetTree(data));
                    }
                }
                self.rebuild_status(&sender);
                root.set_visible(!self.status.is_empty());
            }
            ExecMsg::ChildExited => {
                self.status.clear();
                self.hero = None;
                self.tree = None;
                self.popover.emit(ExecPopoverInput::Clear);
                self.trigger.popdown();
                self.context_menu.popdown();
                self.rebuild_status(&sender);
                root.set_visible(false);
            }
            ExecMsg::Callback(callback) => {
                self.popover
                    .emit(ExecPopoverInput::RememberInteraction(callback.id.clone()));
                if let Err(error) = self.outbound_tx.send(PanelMessage::Callback(callback)) {
                    tracing::warn!(%error, applet = %self.name, "exec applet: failed to queue callback");
                }
            }
            ExecMsg::RestartCommand => {
                self.trigger.popdown();
                self.context_menu.popdown();
                if let Err(error) = self.restart_tx.send(SupervisorControl::Restart) {
                    tracing::warn!(%error, applet = %self.name, "exec applet: failed to request restart");
                }
            }
            ExecMsg::TogglePopover => {
                if self.has_popover_content() {
                    if self.popover.widget().is_visible() {
                        self.trigger.popdown();
                    } else {
                        self.context_menu.popdown();
                        self.trigger.popup();
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

    fn rebuild_status(&self, sender: &ComponentSender<Self>) {
        while let Some(child) = self.status_box.first_child() {
            self.status_box.remove(&child);
        }

        for (index, item) in self.status.iter().enumerate() {
            self.status_box.append(&build_status_item(
                item,
                index,
                self.has_popover_content(),
                sender,
            ));
        }
    }
}

fn build_status_item(
    item: &StatusItem,
    index: usize,
    has_popover: bool,
    sender: &ComponentSender<Exec>,
) -> gtk::Box {
    let fallback_item = item.clone();
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    container.add_css_class("exec-status-item");

    if let Some(icon) = &item.icon {
        let image = gtk::Image::new();
        apply_icon_to_image(&image, icon);
        image.set_pixel_size(16);
        container.append(&image);
    }
    if let Some(text) = &item.text {
        let label = gtk::Label::new(Some(text));
        label.add_css_class("exec-status-label");
        container.append(&label);
    }

    let click_sender = sender.clone();
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        let button_name = mouse_button_name(gesture.current_button());
        let opens_popover = has_popover && gesture.current_button() == 1;
        if opens_popover {
            click_sender.input(ExecMsg::TogglePopover);
        }
        if let Some(callback) =
            status_click_callback(&fallback_item, index, button_name, opens_popover)
        {
            if let PanelMessage::Callback(callback) = callback {
                click_sender.input(ExecMsg::Callback(callback));
            }
        }
    });
    container.add_controller(click);

    let scroll_id = item.id.clone();
    let scroll_sender = sender.clone();
    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
    );
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
    container.add_controller(scroll);

    container
}

fn mouse_button_name(button: u32) -> &'static str {
    match button {
        1 => "left",
        2 => "middle",
        3 => "right",
        _ => "other",
    }
}

fn status_click_callback(
    item: &StatusItem,
    index: usize,
    button: &str,
    opens_popover: bool,
) -> Option<PanelMessage> {
    if opens_popover && button == "left" {
        return None;
    }
    PanelMessage::status_click(item, index, button)
}

async fn run_supervisor(
    name: String,
    config: ExecConfig,
    mut outbound_rx: mpsc::UnboundedReceiver<PanelMessage>,
    mut restart_rx: mpsc::UnboundedReceiver<SupervisorControl>,
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
        let mut restart_now = false;
        loop {
            tokio::select! {
                maybe_restart = restart_rx.recv() => {
                    match maybe_restart {
                        Some(SupervisorControl::Restart) => {
                            tracing::info!(applet = %name, "exec applet: restart requested");
                            restart_now = true;
                            let _ = child.kill().await;
                            break;
                        }
                        None => {
                            should_stop = true;
                            let _ = child.kill().await;
                            break;
                        }
                    }
                }
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
        if !restart_now {
            tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
        }
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
        super::protocol::{CallbackData, InitData, PanelMessage, StatusItem},
        ExecMsg, SupervisorControl, encode_message_line, mouse_button_name, run_supervisor,
        status_click_callback,
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
    fn left_click_that_opens_popover_does_not_also_emit_status_callback() {
        let item = StatusItem {
            id: Some("deploy_status".into()),
            icon: None,
            text: Some("Ready".into()),
        };

        let callback = status_click_callback(&item, 0, "left", true);

        assert_eq!(callback, None);
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
        let (_restart_tx, restart_rx) = tokio::sync::mpsc::unbounded_channel();
        let (sender, receiver) = channel();
        let task = tokio::spawn(run_supervisor(
            "demo".into(),
            ExecConfig {
                command: vec![script_path.to_string_lossy().into_owned()],
                restart_delay_ms: 10_000,
            },
            outbound_rx,
            restart_rx,
            sender,
        ));

        let message = recv_exec_msg(&receiver).await;
        assert!(matches!(
            message,
            ExecMsg::ChildMessage(ChildMessage::Status(StatusData { .. }))
        ));

        let init = fs::read_to_string(&init_path).expect("child should capture init");
        assert_eq!(
            init,
            "{\"type\":\"init\",\"data\":{\"instance\":\"demo\"}}\n"
        );

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
        let (_restart_tx, restart_rx) = tokio::sync::mpsc::unbounded_channel();
        let (sender, receiver) = channel();
        let task = tokio::spawn(run_supervisor(
            "demo".into(),
            ExecConfig {
                command: vec![script_path.to_string_lossy().into_owned()],
                restart_delay_ms: 50,
            },
            outbound_rx,
            restart_rx,
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

    #[tokio::test]
    async fn supervisor_restarts_immediately_when_requested() {
        let temp_dir = make_temp_dir("exec-restart-now");
        let counter_path = temp_dir.join("counter");
        let script_path = temp_dir.join("child.sh");
        write_script(
            &script_path,
            &format!(
                "#!/usr/bin/env bash\ncount=0\nif [ -f {counter} ]; then count=$(cat {counter}); fi\ncount=$((count + 1))\nprintf '%s' \"$count\" > {counter}\nprintf '%s\\n' \"{{\\\"type\\\":\\\"status\\\",\\\"data\\\":{{\\\"items\\\":[{{\\\"id\\\":\\\"demo\\\",\\\"text\\\":\\\"run$count\\\"}}]}}}}\"\nsleep 30\n",
                counter = shell_single_quote(&counter_path)
            ),
        );

        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        let (restart_tx, restart_rx) = tokio::sync::mpsc::unbounded_channel();
        let (sender, receiver) = channel();
        let task = tokio::spawn(run_supervisor(
            "demo".into(),
            ExecConfig {
                command: vec![script_path.to_string_lossy().into_owned()],
                restart_delay_ms: 60_000,
            },
            outbound_rx,
            restart_rx,
            sender,
        ));

        let first = recv_exec_msg(&receiver).await;
        assert_status_text(&first, "run1");

        restart_tx
            .send(SupervisorControl::Restart)
            .expect("restart request should be queued");

        let second = recv_exec_msg(&receiver).await;
        assert!(matches!(second, ExecMsg::ChildExited));

        let third = recv_exec_msg(&receiver).await;
        assert_status_text(&third, "run2");

        drop(outbound_tx);
        drop(restart_tx);
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
        let path = std::env::temp_dir().join(format!("glimpse-{prefix}-{}-{unique}", nanos));
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
