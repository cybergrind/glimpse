#![allow(unused_assignments)]

use std::process::{Output, Stdio};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gio, prelude::*},
};
use serde::Deserialize;
use tokio::process::Command as TokioCommand;

use crate::panels::applets::AppletConfig;

const MAX_LOG_OUTPUT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub icon: Option<String>,
    pub label: Option<String>,
    pub tooltip: Option<String>,
    pub command: Vec<String>,
    pub menu: Vec<MenuItemConfig>,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid command applet config, using defaults");
                Self::default()
            }
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct MenuItemConfig {
    pub label: String,
    pub command: Vec<String>,
}

pub struct Applet {
    name: String,
    config: Config,
    view: View,
    root: gtk::Box,
    context_menu: gtk::PopoverMenu,
}

#[derive(Debug)]
pub struct Init {
    pub name: String,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    Activate,
    MenuCommand(usize),
    ShowContextMenu,
    Reconfigure(Config),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct View {
    visible: bool,
    has_named_icon: bool,
    icon_name: Option<String>,
    has_path_icon: bool,
    icon_path: Option<String>,
    has_label: bool,
    label: String,
    tooltip: Option<String>,
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            set_valign: gtk::Align::Center,
            #[watch]
            set_visible: model.view.visible,
            #[watch]
            set_tooltip_text: model.view.tooltip.as_deref(),

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(Input::Activate);
                },
            },

            add_controller = gtk::GestureClick {
                set_button: 3,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(Input::ShowContextMenu);
                },
            },

            #[name = "named_icon"]
            gtk::Image {
                #[watch]
                set_visible: model.view.has_named_icon,
                #[watch]
                set_icon_name: model.view.icon_name.as_deref(),
                set_pixel_size: 16,
            },

            #[name = "path_icon"]
            gtk::Image {
                #[watch]
                set_visible: model.view.has_path_icon,
                #[watch]
                set_from_file: model.view.icon_path.as_deref(),
                set_pixel_size: 16,
            },

            #[name = "label"]
            gtk::Label {
                #[watch]
                set_visible: model.view.has_label,
                #[watch]
                set_label: &model.view.label,
                set_valign: gtk::Align::Center,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let view = view_from_config(&init.config);
        let context_menu = build_context_menu(&root, &init.config, &sender);
        let model = Applet {
            name: init.name,
            config: init.config,
            view,
            root: root.clone(),
            context_menu,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::Activate => self.spawn_command(&self.config.command),
            Input::MenuCommand(index) => {
                if let Some(item) = self.config.menu.get(index) {
                    self.spawn_command(&item.command);
                    self.context_menu.popdown();
                }
            }
            Input::ShowContextMenu => {
                if has_visible_menu_items(&self.config.menu) {
                    self.context_menu.popup();
                }
            }
            Input::Reconfigure(config) => {
                if self.config == config {
                    return;
                }
                self.context_menu.popdown();
                self.context_menu.unparent();
                self.context_menu = build_context_menu(&self.root, &config, &sender);
                self.view = view_from_config(&config);
                self.config = config;
            }
        }
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.context_menu.popdown();
        self.context_menu.unparent();
    }
}

impl Applet {
    pub fn can_launch(config: &Config) -> bool {
        view_from_config(config).visible
    }

    fn spawn_command(&self, command: &[String]) {
        if command.is_empty() {
            return;
        }

        let name = self.name.clone();
        let command = command.to_vec();
        relm4::spawn(async move {
            if let Err(error) = run_command(&name, command).await {
                tracing::warn!(%error, applet = %name, "command applet command failed to start");
            }
        });
    }
}

async fn run_command(applet: &str, command: Vec<String>) -> anyhow::Result<()> {
    let Some((program, args)) = command.split_first() else {
        return Ok(());
    };

    let output = TokioCommand::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await?;

    log_command_output(applet, program, args, &output);

    Ok(())
}

fn log_command_output(applet: &str, program: &str, args: &[String], output: &Output) {
    let stdout = log_output_text(&output.stdout);
    if !stdout.is_empty() {
        tracing::debug!(
            applet = %applet,
            program = %program,
            args = ?args,
            stream = "stdout",
            output = %stdout,
            "command applet output"
        );
    }

    let stderr = log_output_text(&output.stderr);
    if !stderr.is_empty() {
        tracing::debug!(
            applet = %applet,
            program = %program,
            args = ?args,
            stream = "stderr",
            output = %stderr,
            "command applet output"
        );
    }

    if !output.status.success() {
        tracing::warn!(
            applet = %applet,
            program = %program,
            args = ?args,
            status = %output.status,
            stdout = %stdout,
            stderr = %stderr,
            "command applet command exited with failure"
        );
    }
}

fn log_output_text(bytes: &[u8]) -> String {
    let truncated = bytes.len() > MAX_LOG_OUTPUT_BYTES;
    let bytes = if truncated {
        &bytes[..MAX_LOG_OUTPUT_BYTES]
    } else {
        bytes
    };

    let mut output = String::from_utf8_lossy(bytes).trim_end().to_string();
    if truncated {
        output.push_str("\n... truncated");
    }
    output
}

fn view_from_config(config: &Config) -> View {
    let label = config.label.clone().unwrap_or_default();
    let icon = config.icon.as_deref().filter(|icon| !icon.is_empty());
    let (icon_name, icon_path) = icon
        .map(|icon| {
            if is_icon_path(icon) {
                (None, Some(icon.to_owned()))
            } else {
                (Some(icon.to_owned()), None)
            }
        })
        .unwrap_or((None, None));
    let has_icon = icon_name.is_some() || icon_path.is_some();
    let has_label = !label.is_empty();

    View {
        visible: has_icon || has_label,
        has_named_icon: icon_name.is_some(),
        icon_name,
        has_path_icon: icon_path.is_some(),
        icon_path,
        has_label,
        label,
        tooltip: config.tooltip.clone().filter(|tooltip| !tooltip.is_empty()),
    }
}

fn is_icon_path(icon: &str) -> bool {
    icon.starts_with('/') || icon.starts_with("./") || icon.starts_with("../") || icon.contains('/')
}

fn has_visible_menu_items(menu: &[MenuItemConfig]) -> bool {
    menu.iter()
        .any(|item| !item.label.is_empty() && !item.command.is_empty())
}

fn build_context_menu(
    root: &gtk::Box,
    config: &Config,
    sender: &ComponentSender<Applet>,
) -> gtk::PopoverMenu {
    let action_group = gio::SimpleActionGroup::new();
    let menu = gio::Menu::new();

    for (index, item) in config.menu.iter().enumerate() {
        if item.label.is_empty() || item.command.is_empty() {
            continue;
        }

        let action_name = format!("item-{index}");
        let action = gio::SimpleAction::new(&action_name, None);
        action.connect_activate({
            let sender = sender.input_sender().clone();
            move |_, _| sender.emit(Input::MenuCommand(index))
        });
        action_group.add_action(&action);
        menu.append(Some(&item.label), Some(&format!("command.{action_name}")));
    }

    root.insert_action_group("command", Some(&action_group));
    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(root);
    popover.set_has_arrow(false);
    popover
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_accepts_empty_settings() {
        assert_eq!(Config::from_raw(&None), Config::default());
    }

    #[test]
    fn empty_command_config_does_not_launch() {
        assert!(!Applet::can_launch(&Config::default()));
    }

    #[test]
    fn icon_only_config_can_launch() {
        assert!(Applet::can_launch(&Config {
            icon: Some("camera-photo-symbolic".into()),
            ..Config::default()
        }));
    }

    #[test]
    fn label_only_config_can_launch() {
        assert!(Applet::can_launch(&Config {
            label: Some("Shot".into()),
            ..Config::default()
        }));
    }

    #[test]
    fn view_splits_icon_names_and_paths() {
        let named = view_from_config(&Config {
            icon: Some("camera-photo-symbolic".into()),
            ..Config::default()
        });
        assert!(named.has_named_icon);
        assert_eq!(named.icon_name.as_deref(), Some("camera-photo-symbolic"));
        assert!(!named.has_path_icon);
        assert_eq!(named.icon_path, None);

        let path = view_from_config(&Config {
            icon: Some("/tmp/icon.png".into()),
            ..Config::default()
        });
        assert!(!path.has_named_icon);
        assert_eq!(path.icon_name, None);
        assert!(path.has_path_icon);
        assert_eq!(path.icon_path.as_deref(), Some("/tmp/icon.png"));
    }

    #[test]
    fn menu_visibility_ignores_empty_items() {
        assert!(!has_visible_menu_items(&[MenuItemConfig {
            label: "Open".into(),
            command: Vec::new(),
        }]));
        assert!(has_visible_menu_items(&[MenuItemConfig {
            label: "Open".into(),
            command: vec!["true".into()],
        }]));
    }

    #[test]
    fn log_output_text_trims_and_truncates() {
        assert_eq!(log_output_text(b"hello\n"), "hello");

        let long = vec![b'x'; MAX_LOG_OUTPUT_BYTES + 1];
        let output = log_output_text(&long);
        assert!(output.ends_with("\n... truncated"));
        assert!(output.len() > MAX_LOG_OUTPUT_BYTES);
    }

    #[test]
    fn empty_tooltip_is_ignored() {
        let view = view_from_config(&Config {
            icon: Some("camera-photo-symbolic".into()),
            tooltip: Some(String::new()),
            ..Config::default()
        });

        assert_eq!(view.tooltip, None);
    }
}
