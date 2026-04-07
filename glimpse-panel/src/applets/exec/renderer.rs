use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use relm4::gtk::{self, glib, prelude::*};

use super::protocol::{
    AlignValue, BoxNode, ButtonNode, CallbackData, CheckboxNode, CommonProps, DropdownNode,
    EntryNode, GridNode, IconSource, ImageNode, LabelNode, OrientationValue, ScaleNode,
    SeparatorNode, SwitchNode, TreeNode,
};

pub type CallbackSink = Rc<dyn Fn(CallbackData)>;

#[derive(Clone)]
pub struct RenderCatalog {
    callback: CallbackSink,
    focus_targets: Rc<RefCell<HashMap<String, gtk::Widget>>>,
}

impl Default for RenderCatalog {
    fn default() -> Self {
        Self {
            callback: Rc::new(|_| {}),
            focus_targets: Rc::new(RefCell::new(HashMap::new())),
        }
    }
}

impl RenderCatalog {
    pub fn with_callback(callback: CallbackSink) -> Self {
        Self {
            callback,
            focus_targets: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn focus_targets(&self) -> HashMap<String, gtk::Widget> {
        self.focus_targets.borrow().clone()
    }

    pub fn render(&self, node: &TreeNode) -> Result<gtk::Widget, RenderError> {
        match node {
            TreeNode::Box(data) => self.render_box(data),
            TreeNode::Grid(data) => self.render_grid(data),
            TreeNode::Scroll(data) => {
                let scrolled = gtk::ScrolledWindow::new();
                apply_common_props(&scrolled, &data.common);
                let child = self.render(&data.child)?;
                scrolled.set_child(Some(&child));
                Ok(scrolled.upcast())
            }
            TreeNode::Separator(data) => self.render_separator(data),
            TreeNode::Label(data) => self.render_label(data),
            TreeNode::Image(data) => self.render_image(data),
            TreeNode::Button(data) => self.render_button(data),
            TreeNode::Entry(data) => self.render_entry(data, true),
            TreeNode::Password(data) => self.render_entry(data, false),
            TreeNode::Switch(data) => self.render_switch(data),
            TreeNode::Scale(data) => self.render_scale(data),
            TreeNode::Dropdown(data) => self.render_dropdown(data),
            TreeNode::Checkbox(data) => self.render_checkbox(data),
        }
    }

    fn render_box(&self, data: &BoxNode) -> Result<gtk::Widget, RenderError> {
        let widget = gtk::Box::new(to_orientation(data.orientation), data.spacing);
        apply_common_props(&widget, &data.common);
        for child in &data.children {
            widget.append(&self.render(child)?);
        }
        Ok(widget.upcast())
    }

    fn render_grid(&self, data: &GridNode) -> Result<gtk::Widget, RenderError> {
        let grid = gtk::Grid::new();
        apply_common_props(&grid, &data.common);
        grid.set_row_spacing(data.row_spacing as u32);
        grid.set_column_spacing(data.column_spacing as u32);
        for child in &data.children {
            let rendered = self.render(&child.child)?;
            grid.attach(
                &rendered,
                child.column,
                child.row,
                child.width,
                child.height,
            );
        }
        Ok(grid.upcast())
    }

    fn render_label(&self, data: &LabelNode) -> Result<gtk::Widget, RenderError> {
        let label = gtk::Label::new(Some(&data.text));
        apply_common_props(&label, &data.common);
        label.set_wrap(data.wrap);
        label.set_selectable(data.selectable);
        if let Some(xalign) = data.xalign {
            label.set_xalign(xalign);
        }
        Ok(label.upcast())
    }

    fn render_image(&self, data: &ImageNode) -> Result<gtk::Widget, RenderError> {
        let image = gtk::Image::new();
        apply_common_props(&image, &data.common);
        apply_icon_to_image(&image, &data.icon);
        if let Some(pixel_size) = data.pixel_size {
            image.set_pixel_size(pixel_size);
        }
        Ok(image.upcast())
    }

    fn render_button(&self, data: &ButtonNode) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("button", &data.common)?;
        let button = gtk::Button::new();
        apply_common_props(&button, &data.common);
        button.add_css_class("flat");
        if let Some(child) = &data.child {
            button.set_child(Some(&self.render(child)?));
        } else {
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            if let Some(icon) = &data.icon {
                let image = gtk::Image::new();
                apply_icon_to_image(&image, icon);
                content.append(&image);
            }
            if let Some(label) = &data.label {
                content.append(&gtk::Label::new(Some(label)));
            }
            button.set_child(Some(&content));
        }

        let callback = self.callback.clone();
        let callback_id = id.clone();
        button.connect_clicked(move |_| {
            callback(CallbackData {
                id: callback_id.clone(),
                event: "click".into(),
                button: Some("left".into()),
                ..CallbackData::default()
            });
        });
        self.register_focus_target(&id, &button);
        Ok(button.upcast())
    }

    fn render_entry(&self, data: &EntryNode, visible: bool) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("entry", &data.common)?;
        let entry = gtk::Entry::new();
        apply_common_props(&entry, &data.common);
        entry.set_text(&data.text);
        entry.set_visibility(visible);
        if let Some(placeholder) = &data.placeholder {
            entry.set_placeholder_text(Some(placeholder));
        }
        let callback = self.callback.clone();
        let debounce = Rc::new(RefCell::new(None));
        let callback_id = id.clone();
        entry.connect_changed(move |entry| {
            emit_callback(
                &callback,
                &debounce,
                CallbackData {
                    id: callback_id.clone(),
                    event: "input".into(),
                    text: Some(entry.text().to_string()),
                    ..CallbackData::default()
                },
            );
        });
        self.register_focus_target(&id, &entry);
        Ok(entry.upcast())
    }

    fn render_switch(&self, data: &SwitchNode) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("switch", &data.common)?;
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        apply_common_props(&row, &data.common);
        if let Some(label) = &data.label {
            let text = gtk::Label::new(Some(label));
            text.set_hexpand(true);
            text.set_halign(gtk::Align::Start);
            row.append(&text);
        }
        let switch = gtk::Switch::new();
        switch.set_active(data.active);
        let callback = self.callback.clone();
        let callback_id = id.clone();
        switch.connect_active_notify(move |switch| {
            callback(CallbackData {
                id: callback_id.clone(),
                event: "toggle".into(),
                value: Some(serde_json::Value::Bool(switch.is_active())),
                ..CallbackData::default()
            });
        });
        row.append(&switch);
        self.register_focus_target(&id, &switch);
        Ok(row.upcast())
    }

    fn render_scale(&self, data: &ScaleNode) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("scale", &data.common)?;
        let scale = gtk::Scale::with_range(
            to_orientation(data.orientation.unwrap_or(OrientationValue::Horizontal)),
            data.min,
            data.max,
            data.step,
        );
        apply_common_props(&scale, &data.common);
        scale.set_draw_value(data.draw_value);
        scale.set_value(data.value);
        let callback = self.callback.clone();
        let debounce = Rc::new(RefCell::new(None));
        let callback_id = id.clone();
        scale.connect_value_changed(move |scale| {
            emit_callback(
                &callback,
                &debounce,
                CallbackData {
                    id: callback_id.clone(),
                    event: "change".into(),
                    value: Some(serde_json::Value::from(scale.value())),
                    ..CallbackData::default()
                },
            );
        });
        self.register_focus_target(&id, &scale);
        Ok(scale.upcast())
    }

    fn render_separator(&self, data: &SeparatorNode) -> Result<gtk::Widget, RenderError> {
        let separator = gtk::Separator::new(to_orientation(
            data.orientation.unwrap_or(OrientationValue::Horizontal),
        ));
        apply_common_props(&separator, &data.common);
        Ok(separator.upcast())
    }

    fn render_checkbox(&self, data: &CheckboxNode) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("checkbox", &data.common)?;
        let checkbox = if let Some(label) = &data.label {
            gtk::CheckButton::with_label(label)
        } else {
            gtk::CheckButton::new()
        };
        apply_common_props(&checkbox, &data.common);
        checkbox.set_active(data.active);
        let callback = self.callback.clone();
        let callback_id = id.clone();
        checkbox.connect_toggled(move |button| {
            callback(CallbackData {
                id: callback_id.clone(),
                event: "toggle".into(),
                value: Some(serde_json::Value::Bool(button.is_active())),
                ..CallbackData::default()
            });
        });
        self.register_focus_target(&id, &checkbox);
        Ok(checkbox.upcast())
    }

    fn render_dropdown(&self, data: &DropdownNode) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("dropdown", &data.common)?;
        let labels: Vec<&str> = data.items.iter().map(|item| item.label.as_str()).collect();
        let dropdown = gtk::DropDown::from_strings(&labels);
        apply_common_props(&dropdown, &data.common);
        if let Some(selected) = data.selected {
            dropdown.set_selected(selected);
        }
        let items = data.items.clone();
        let callback = self.callback.clone();
        let debounce = Rc::new(RefCell::new(None));
        let callback_id = id.clone();
        dropdown.connect_selected_notify(move |dropdown| {
            let index = dropdown.selected();
            let value = items
                .get(index as usize)
                .map(|item| serde_json::json!({"id": item.id, "label": item.label, "index": index}))
                .unwrap_or_else(|| serde_json::json!({"index": index}));
            emit_callback(
                &callback,
                &debounce,
                CallbackData {
                    id: callback_id.clone(),
                    event: "change".into(),
                    value: Some(value),
                    ..CallbackData::default()
                },
            );
        });
        self.register_focus_target(&id, &dropdown);
        Ok(dropdown.upcast())
    }

    fn register_focus_target(&self, id: &str, widget: &impl IsA<gtk::Widget>) {
        self.focus_targets
            .borrow_mut()
            .insert(id.to_owned(), widget.clone().upcast());
    }
}

#[derive(Debug)]
pub enum RenderError {
    MissingInteractiveId { widget_type: &'static str },
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::MissingInteractiveId { widget_type } => {
                write!(f, "{widget_type} widgets require an id for callbacks")
            }
        }
    }
}

pub fn apply_icon_to_image(image: &gtk::Image, icon: &IconSource) {
    match icon {
        IconSource::Name(name) => image.set_icon_name(Some(name)),
        IconSource::Path(path) => image.set_from_file(Some(path)),
    }
}

fn apply_common_props(widget: &impl IsA<gtk::Widget>, props: &CommonProps) {
    if let Some(visible) = props.visible {
        widget.set_visible(visible);
    }
    if let Some(hexpand) = props.hexpand {
        widget.set_hexpand(hexpand);
    }
    if let Some(vexpand) = props.vexpand {
        widget.set_vexpand(vexpand);
    }
    if let Some(halign) = props.halign {
        widget.set_halign(to_align(halign));
    }
    if let Some(valign) = props.valign {
        widget.set_valign(to_align(valign));
    }
    if let Some(tooltip) = &props.tooltip {
        widget.set_tooltip_text(Some(tooltip));
    }
    for class_name in &props.css_classes {
        widget.add_css_class(class_name);
    }
}

fn require_interactive_id(
    widget_type: &'static str,
    props: &CommonProps,
) -> Result<String, RenderError> {
    props
        .id
        .clone()
        .ok_or(RenderError::MissingInteractiveId { widget_type })
}

fn to_align(value: AlignValue) -> gtk::Align {
    match value {
        AlignValue::Fill => gtk::Align::Fill,
        AlignValue::Start => gtk::Align::Start,
        AlignValue::End => gtk::Align::End,
        AlignValue::Center => gtk::Align::Center,
        AlignValue::Baseline => gtk::Align::Baseline,
    }
}

fn to_orientation(value: OrientationValue) -> gtk::Orientation {
    match value {
        OrientationValue::Horizontal => gtk::Orientation::Horizontal,
        OrientationValue::Vertical => gtk::Orientation::Vertical,
    }
}

fn emit_callback(
    callback: &CallbackSink,
    debounce: &Rc<RefCell<Option<glib::SourceId>>>,
    data: CallbackData,
) {
    if let Some(delay_ms) = callback_debounce_delay_ms(&data.event) {
        if let Some(source_id) = debounce.borrow_mut().take() {
            source_id.remove();
        }
        let callback = callback.clone();
        let debounce = debounce.clone();
        let clear_handle = debounce.clone();
        *debounce.borrow_mut() = Some(glib::timeout_add_local_once(
            Duration::from_millis(delay_ms),
            move || {
                callback(data);
                clear_handle.borrow_mut().take();
            },
        ));
    } else {
        callback(data);
    }
}

fn callback_debounce_delay_ms(event: &str) -> Option<u64> {
    match event {
        "input" | "change" => Some(300),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use relm4::gtk;
    use relm4::gtk::prelude::ObjectExt;

    use super::{RenderCatalog, RenderError};
    use crate::applets::exec::protocol::TreeNode;

    #[test]
    fn renderer_rejects_unknown_widget_types() {
        let value = serde_json::json!({
            "type": "unknown_widget",
            "data": {}
        });

        let err = serde_json::from_value::<TreeNode>(value).expect_err("node should fail to parse");

        assert!(err.to_string().contains("unknown_widget"));
    }

    #[test]
    fn renderer_builds_label_nodes() {
        if gtk::init().is_err() {
            return;
        }
        let node = serde_json::from_value::<TreeNode>(serde_json::json!({
            "type": "label",
            "data": {
                "text": "Connected",
                "css_classes": ["dim-label"]
            }
        }))
        .expect("label should parse");

        let widget = RenderCatalog::default()
            .render(&node)
            .expect("label should render");

        assert!(widget.is::<gtk::Label>());
    }

    #[test]
    fn renderer_requires_ids_for_interactive_nodes() {
        let node = serde_json::from_value::<TreeNode>(serde_json::json!({
            "type": "button",
            "data": {
                "label": "Refresh"
            }
        }))
        .expect("button should parse");

        let err = RenderCatalog::default()
            .render(&node)
            .expect_err("interactive node should require an id");

        assert!(matches!(err, RenderError::MissingInteractiveId { .. }));
    }

    #[test]
    fn render_error_display_mentions_widget_type() {
        let err = RenderError::MissingInteractiveId {
            widget_type: "button",
        };

        assert_eq!(
            err.to_string(),
            "button widgets require an id for callbacks"
        );
    }

    #[test]
    fn callback_debounce_policy_delays_input_and_change() {
        assert_eq!(super::callback_debounce_delay_ms("input"), Some(300));
        assert_eq!(super::callback_debounce_delay_ms("change"), Some(300));
    }

    #[test]
    fn callback_debounce_policy_keeps_click_and_toggle_immediate() {
        assert_eq!(super::callback_debounce_delay_ms("click"), None);
        assert_eq!(super::callback_debounce_delay_ms("toggle"), None);
    }

    #[test]
    fn renderer_registers_focus_targets_for_entries() {
        if gtk::init().is_err() {
            return;
        }
        let node = serde_json::from_value::<TreeNode>(serde_json::json!({
            "type": "entry",
            "data": {
                "id": "version",
                "text": "v1"
            }
        }))
        .expect("entry should parse");

        let renderer = RenderCatalog::default();
        let _ = renderer.render(&node).expect("entry should render");

        assert!(renderer.focus_targets().contains_key("version"));
    }
}
