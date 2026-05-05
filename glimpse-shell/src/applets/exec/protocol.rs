use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Icon {
    Name { name: String },
    Path { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StatusItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StatusPayload {
    #[serde(default)]
    pub items: Vec<StatusItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PopoverPayload {
    #[serde(default)]
    pub root: Option<TreeNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChildCommand {
    Status(StatusPayload),
    Popover(PopoverPayload),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum PanelCommand {
    Init(InitPayload),
    Event(EventPayload),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InitPayload {
    pub instance: String,
    pub options: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventPayload {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: EventKind,
    pub source: EventSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button: Option<MouseButton>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_y: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Click,
    Toggle,
    Change,
    Scroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Status,
    Popover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Other,
}

impl MouseButton {
    pub fn from_number(button: u32) -> Self {
        match button {
            1 => Self::Left,
            2 => Self::Middle,
            3 => Self::Right,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CommonProps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hexpand: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vexpand: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halign: Option<AlignValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valign: Option<AlignValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Normal,
    Muted,
    Accent,
    Success,
    Warning,
    Danger,
}

impl Variant {
    pub fn class_name(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::Muted => Some("is-muted"),
            Self::Accent => Some("is-accent"),
            Self::Success => Some("is-success"),
            Self::Warning => Some("is-warning"),
            Self::Danger => Some("is-danger"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlignValue {
    Fill,
    Start,
    End,
    Center,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrientationValue {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeroNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub icon: Option<Icon>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoxNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub orientation: OrientationValue,
    #[serde(default)]
    pub spacing: i32,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollapsibleSectionNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    #[serde(default)]
    pub expanded: bool,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RowNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub meta: String,
    #[serde(default)]
    pub icon: Option<Icon>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionMenuItemNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon: Option<Icon>,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub checked: Option<bool>,
    #[serde(default)]
    pub selectable: Option<bool>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionMenuNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub header: Option<String>,
    #[serde(default)]
    pub items: Vec<ActionMenuItemNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetailGridItem {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetailGridNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub rows: Vec<DetailGridItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmptyStateNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BadgeNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusDotNode {
    #[serde(flatten)]
    pub common: CommonProps,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceStatusNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub busy: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub text: String,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub xalign: Option<f32>,
    #[serde(default)]
    pub selectable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IconNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub icon: Icon,
    #[serde(default)]
    pub pixel_size: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub icon: Icon,
    #[serde(default)]
    pub pixel_size: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ButtonNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub icon: Option<Icon>,
    #[serde(default)]
    pub child: Option<Box<TreeNode>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwitchNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckboxNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScaleNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub value: f64,
    #[serde(default)]
    pub orientation: Option<OrientationValue>,
    #[serde(default)]
    pub draw_value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DropdownItem {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropdownNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub items: Vec<DropdownItem>,
    #[serde(default)]
    pub selected: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgressNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub value: f64,
    #[serde(default = "default_progress_max")]
    pub max: f64,
    #[serde(default)]
    pub show_text: bool,
    #[serde(default)]
    pub text: Option<String>,
}

fn default_progress_max() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeparatorNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub orientation: Option<OrientationValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScrollNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub child: Box<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridChildNode {
    pub row: i32,
    pub column: i32,
    #[serde(default = "default_grid_span")]
    pub width: i32,
    #[serde(default = "default_grid_span")]
    pub height: i32,
    pub child: TreeNode,
}

fn default_grid_span() -> i32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub row_spacing: i32,
    #[serde(default)]
    pub column_spacing: i32,
    #[serde(default)]
    pub children: Vec<GridChildNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TreeNode {
    Hero(HeroNode),
    Card(CardNode),
    Section(SectionNode),
    CollapsibleSection(CollapsibleSectionNode),
    ActionMenu(ActionMenuNode),
    Row(RowNode),
    DetailGrid(DetailGridNode),
    EmptyState(EmptyStateNode),
    Badge(BadgeNode),
    StatusDot(StatusDotNode),
    DeviceStatus(DeviceStatusNode),
    Box(BoxNode),
    Grid(GridNode),
    Scroll(ScrollNode),
    Progress(ProgressNode),
    Separator(SeparatorNode),
    Label(LabelNode),
    Icon(IconNode),
    Image(ImageNode),
    Button(ButtonNode),
    Switch(SwitchNode),
    Checkbox(CheckboxNode),
    Scale(ScaleNode),
    Dropdown(DropdownNode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    MissingCommand,
    MissingPayload { command: String },
    UnknownCommand { command: String },
    InvalidJson { command: String, message: String },
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingCommand => write!(f, "missing command"),
            Self::MissingPayload { command } => write!(f, "{command}: missing JSON payload"),
            Self::UnknownCommand { command } => write!(f, "unknown exec command {command}"),
            Self::InvalidJson { command, message } => {
                write!(f, "{command}: invalid JSON payload: {message}")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

pub fn parse_child_line(line: &str) -> Result<ChildCommand, ProtocolError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(ProtocolError::MissingCommand);
    }

    let (command, payload) = split_line(line)?;
    match command {
        "status" => decode_payload(command, payload).map(ChildCommand::Status),
        "popover" => decode_payload(command, payload).map(ChildCommand::Popover),
        other => Err(ProtocolError::UnknownCommand {
            command: other.into(),
        }),
    }
}

pub fn encode_panel_command(command: &PanelCommand) -> String {
    let (name, payload) = match command {
        PanelCommand::Init(payload) => (
            "init",
            serde_json::to_string(payload).expect("init payload should serialize"),
        ),
        PanelCommand::Event(payload) => (
            "event",
            serde_json::to_string(payload).expect("event payload should serialize"),
        ),
    };
    format!("{name} {payload}")
}

fn split_line(line: &str) -> Result<(&str, &str), ProtocolError> {
    let mut parts = line.splitn(2, char::is_whitespace);
    let command = parts.next().filter(|command| !command.is_empty());
    let payload = parts.next().map(str::trim_start);
    match (command, payload) {
        (Some(command), Some(payload)) if !payload.is_empty() => Ok((command, payload)),
        (Some(command), _) => Err(ProtocolError::MissingPayload {
            command: command.into(),
        }),
        _ => Err(ProtocolError::MissingCommand),
    }
}

fn decode_payload<'de, T>(command: &str, payload: &'de str) -> Result<T, ProtocolError>
where
    T: Deserialize<'de>,
{
    serde_json::from_str(payload).map_err(|error| ProtocolError::InvalidJson {
        command: command.into(),
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_line_with_object_icons_and_labels() {
        let command = parse_child_line(
            r#"status {"items":[{"id":"cpu","icon":{"name":"cpu-symbolic"},"label":"12%","tooltip":"CPU usage"}]}"#,
        )
        .expect("status line should parse");

        assert_eq!(
            command,
            ChildCommand::Status(StatusPayload {
                items: vec![StatusItem {
                    id: Some("cpu".into()),
                    icon: Some(Icon::Name {
                        name: "cpu-symbolic".into(),
                    }),
                    label: Some("12%".into()),
                    tooltip: Some("CPU usage".into()),
                }]
            })
        );
    }

    #[test]
    fn parses_popover_line_with_root_node() {
        let command = parse_child_line(
            r#"popover {"root":{"type":"section","data":{"title":"System","children":[{"type":"button","data":{"id":"refresh","label":"Refresh"}}]}}}"#,
        )
        .expect("popover line should parse");

        assert!(matches!(
            command,
            ChildCommand::Popover(PopoverPayload {
                root: Some(TreeNode::Section(_))
            })
        ));
    }

    #[test]
    fn rejects_text_entry_nodes() {
        let error = parse_child_line(
            r#"popover {"root":{"type":"entry","data":{"id":"name","text":"bad"}}}"#,
        )
        .expect_err("text entry nodes should be unsupported");

        assert!(error.to_string().contains("popover"));
    }

    #[test]
    fn encodes_init_and_event_lines() {
        assert_eq!(
            encode_panel_command(&PanelCommand::Init(InitPayload {
                instance: "sysinfo".into(),
                options: serde_json::json!({"interval": 5}),
            })),
            r#"init {"instance":"sysinfo","options":{"interval":5}}"#.to_string()
        );

        assert_eq!(
            encode_panel_command(&PanelCommand::Event(EventPayload {
                id: "refresh".into(),
                kind: EventKind::Click,
                source: EventSource::Popover,
                button: Some(MouseButton::Left),
                active: None,
                value: None,
                delta_y: None,
            })),
            r#"event {"id":"refresh","type":"click","source":"popover","button":"left"}"#
                .to_string()
        );
    }
}
