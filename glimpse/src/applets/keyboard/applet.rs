#![allow(unused_assignments)]

use glimpse::compositor::{
    KeyboardLayoutCommand, KeyboardLayoutServiceHandle, KeyboardLayoutState, short_layout_name,
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk,
};

use super::components::layout_label::{
    KeyboardLayoutLabel, KeyboardLayoutLabelInput, KeyboardLayoutLabelOutput, KeyboardLayoutView,
};
use super::{KeyboardConfig, KeyboardFormat};

pub struct Keyboard {
    config: KeyboardConfig,
    service: KeyboardLayoutServiceHandle,
    indicator: Controller<KeyboardLayoutLabel>,
}

pub struct KeyboardInit {
    pub config: KeyboardConfig,
    pub service: KeyboardLayoutServiceHandle,
}

#[derive(Debug, Clone)]
pub enum KeyboardInput {
    ServiceState(KeyboardLayoutState),
    IndicatorOutput(KeyboardLayoutLabelOutput),
}

#[relm4::component(pub)]
impl Component for Keyboard {
    type Init = KeyboardInit;
    type Input = KeyboardInput;
    type Output = ();
    type CommandOutput = KeyboardInput;

    view! {
        gtk::Box {
            #[local_ref]
            indicator_widget -> gtk::Box {},
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let indicator = KeyboardLayoutLabel::builder()
            .launch(())
            .forward(sender.input_sender(), KeyboardInput::IndicatorOutput);
        let indicator_widget = indicator.widget().clone();
        let per_window = init.config.per_window;
        let service = init.service.fork(per_window);

        let model = Keyboard {
            config: init.config,
            service,
            indicator,
        };
        let service = model.service.clone();
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    let mut state_rx = service.subscribe();
                    let _ = out.send(KeyboardInput::ServiceState(state_rx.borrow().clone()));

                    while state_rx.changed().await.is_ok() {
                        let _ = out.send(KeyboardInput::ServiceState(state_rx.borrow().clone()));
                    }
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(message, sender, root);
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            KeyboardInput::ServiceState(state) => {
                let full_name = state
                    .snapshot
                    .layout_names
                    .get(state.snapshot.current_index)
                    .cloned()
                    .unwrap_or_default();

                let display = if let Some(label) = self.config.labels.get(&full_name) {
                    label.clone()
                } else {
                    match self.config.format {
                        KeyboardFormat::Short => short_layout_name(&full_name),
                        KeyboardFormat::Full => full_name.clone(),
                    }
                };

                self.indicator
                    .emit(KeyboardLayoutLabelInput::Update(KeyboardLayoutView {
                        label: display,
                        tooltip: full_name,
                    }));
            }
            KeyboardInput::IndicatorOutput(KeyboardLayoutLabelOutput::Scroll(dy)) => {
                self.send_command(sender, KeyboardLayoutCommand::SwitchRelative(dy > 0.0));
            }
        }
    }
}

impl Keyboard {
    fn send_command(&self, sender: ComponentSender<Self>, command: KeyboardLayoutCommand) {
        let service = self.service.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    if let Err(error) = service.send(command).await {
                        tracing::warn!(error = %error, "keyboard applet: failed to send layout command");
                    }
                })
                .drop_on_shutdown()
        });
    }
}
