use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use glimpse::providers::audio::DeviceList;

use super::device_list_section::{
    DeviceListSection, DeviceListSectionInit, DeviceListSectionInput, DeviceListSectionOutput,
};

pub struct DeviceSection {
    outputs: Controller<DeviceListSection>,
    inputs: Controller<DeviceListSection>,
}

pub struct DeviceSectionInit;

#[derive(Debug)]
pub enum DeviceSectionInput {
    UpdateOutputs(DeviceList),
    UpdateInputs(DeviceList),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceSectionOutput {
    SetDefaultOutput(String),
    SetDefaultInput(String),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for DeviceSection {
    type Init = DeviceSectionInit;
    type Input = DeviceSectionInput;
    type Output = DeviceSectionOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,

            #[local_ref]
            outputs_widget -> gtk::Box {},

            #[local_ref]
            inputs_widget -> gtk::Box {},
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let outputs = DeviceListSection::builder()
            .launch(DeviceListSectionInit {
                title: "Output device".into(),
            })
            .forward(sender.output_sender(), |output| match output {
                DeviceListSectionOutput::Selected(name) => {
                    DeviceSectionOutput::SetDefaultOutput(name)
                }
            });

        let inputs = DeviceListSection::builder()
            .launch(DeviceListSectionInit {
                title: "Input device".into(),
            })
            .forward(sender.output_sender(), |output| match output {
                DeviceListSectionOutput::Selected(name) => {
                    DeviceSectionOutput::SetDefaultInput(name)
                }
            });

        let outputs_widget = outputs.widget().clone();
        let inputs_widget = inputs.widget().clone();

        let widgets = view_output!();
        let model = DeviceSection { outputs, inputs };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            DeviceSectionInput::UpdateOutputs(outputs) => {
                self.outputs.emit(DeviceListSectionInput::Update(outputs));
            }
            DeviceSectionInput::UpdateInputs(inputs) => {
                self.inputs.emit(DeviceListSectionInput::Update(inputs));
            }
        }
    }
}
