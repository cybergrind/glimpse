use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, prelude::*},
};
use tokio::sync::mpsc;

use super::compositor::{self, Compositor, KeyboardState};
use super::config::{KeyboardConfig, KeyboardFormat};

pub struct Keyboard {
    config: KeyboardConfig,
    compositor: Option<Compositor>,
    action_tx: mpsc::Sender<KeyboardAction>,
    label: gtk::Label,
}

pub struct KeyboardInit {
    pub config: KeyboardConfig,
}

#[derive(Debug)]
pub enum KeyboardInput {
    Scroll(f64),
}

#[derive(Debug)]
enum KeyboardAction {
    SwitchRelative(bool),
}

#[relm4::component(pub)]
impl Component for Keyboard {
    type Init = KeyboardInit;
    type Input = KeyboardInput;
    type Output = ();
    type CommandOutput = KeyboardState;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "applet",
            add_css_class: "keyboard",
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let compositor = compositor::detect();
        let (action_tx, action_rx) = mpsc::channel::<KeyboardAction>(16);

        let label = gtk::Label::new(None);
        label.add_css_class("keyboard-label");
        root.append(&label);

        let model = Keyboard {
            config: init.config.clone(),
            compositor,
            action_tx,
            label,
        };
        let widgets = view_output!();

        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let scroll_sender = sender.clone();
        scroll.connect_scroll(move |_, _, dy| {
            scroll_sender.input(KeyboardInput::Scroll(dy));
            gtk::glib::Propagation::Stop
        });
        root.add_controller(scroll);

        if let Some(comp) = compositor {
            let per_window = init.config.per_window;
            sender.command(move |cmd_tx, shutdown| {
                shutdown
                    .register(async move {
                        let (state_tx, mut state_rx) = mpsc::channel::<KeyboardState>(16);
                        let mut action_rx = action_rx;

                        let event_handle = tokio::spawn(async move {
                            match comp {
                                Compositor::Hyprland => {
                                    compositor::hyprland_event_loop(state_tx, per_window).await;
                                }
                                Compositor::Niri => {
                                    compositor::niri_event_loop(state_tx, per_window).await;
                                }
                            }
                        });

                        loop {
                            tokio::select! {
                                Some(state) = state_rx.recv() => {
                                    if cmd_tx.send(state).is_err() {
                                        break;
                                    }
                                }
                                Some(action) = action_rx.recv() => {
                                    match action {
                                        KeyboardAction::SwitchRelative(next) => {
                                            compositor::switch_layout_relative(comp, next).await;
                                        }
                                    }
                                }
                                else => break,
                            }
                        }

                        event_handle.abort();
                    })
                    .drop_on_shutdown()
            });
        } else {
            tracing::warn!("keyboard: no supported compositor detected");
        }

        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        state: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        let full_name = state
            .layout_names
            .get(state.current_index)
            .cloned()
            .unwrap_or_default();

        let display = if let Some(label) = self.config.labels.get(&full_name) {
            label.clone()
        } else {
            match self.config.format {
                KeyboardFormat::Short => compositor::short_name(&full_name),
                KeyboardFormat::Full => full_name.clone(),
            }
        };

        self.label.set_label(&display);
        root.set_tooltip_text(Some(&full_name));
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            KeyboardInput::Scroll(dy) => {
                let next = dy > 0.0;
                self.action_tx
                    .try_send(KeyboardAction::SwitchRelative(next))
                    .ok();
            }
        }
    }
}
