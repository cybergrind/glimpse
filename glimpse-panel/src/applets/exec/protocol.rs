use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum IconSource {
    Name(String),
    Path(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusItem {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub icon: Option<IconSource>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusData {
    #[serde(default)]
    pub items: Vec<StatusItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeroNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    pub subtitle: String,
    #[serde(default)]
    pub icon: Option<IconSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CommonProps {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub visible: Option<bool>,
    #[serde(default)]
    pub hexpand: Option<bool>,
    #[serde(default)]
    pub vexpand: Option<bool>,
    #[serde(default)]
    pub halign: Option<AlignValue>,
    #[serde(default)]
    pub valign: Option<AlignValue>,
    #[serde(default)]
    pub tooltip: Option<String>,
    #[serde(default)]
    pub css_classes: Vec<String>,
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
pub struct ImageNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub icon: IconSource,
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
    pub icon: Option<IconSource>,
    #[serde(default)]
    pub child: Option<Box<TreeNode>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub placeholder: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeparatorNode {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(default)]
    pub orientation: Option<OrientationValue>,
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
pub struct DropdownItem {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DropdownNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub items: Vec<DropdownItem>,
    #[serde(default)]
    pub selected: Option<u32>,
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
    #[serde(default = "default_span")]
    pub width: i32,
    #[serde(default = "default_span")]
    pub height: i32,
    pub child: TreeNode,
}

fn default_span() -> i32 {
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
    Box(BoxNode),
    Grid(GridNode),
    Scroll(ScrollNode),
    Separator(SeparatorNode),
    Label(LabelNode),
    Image(ImageNode),
    Button(ButtonNode),
    Entry(EntryNode),
    Password(EntryNode),
    Switch(SwitchNode),
    Scale(ScaleNode),
    Dropdown(DropdownNode),
    Checkbox(CheckboxNode),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChildMessage {
    Status(StatusData),
    Tree {
        content: Option<TreeNode>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum PanelMessage {
    Init(InitData),
    Callback(CallbackData),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InitData {
    pub instance: String,
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct CallbackData {
    pub id: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_y: Option<f64>,
}

impl PanelMessage {
    pub fn status_click(item: &StatusItem, index: usize, button: &str) -> Option<Self> {
        let id = item.id.clone().unwrap_or_else(|| index.to_string());
        Some(Self::Callback(CallbackData {
            id,
            event: "click".into(),
            button: Some(button.into()),
            ..CallbackData::default()
        }))
    }
}

#[derive(Debug, Deserialize)]
struct RawMessage {
    #[serde(rename = "type")]
    kind: String,
    data: Value,
}

impl<'de> Deserialize<'de> for ChildMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawMessage::deserialize(deserializer)?;
        match raw.kind.as_str() {
            "status" => serde_json::from_value(raw.data)
                .map(ChildMessage::Status)
                .map_err(serde::de::Error::custom),
            "tree" => {
                #[derive(Deserialize, Default)]
                struct TreePayload {
                    #[serde(default)]
                    content: Option<TreeNode>,
                }

                let payload: TreePayload = if raw.data.is_null() {
                    TreePayload::default()
                } else {
                    serde_json::from_value(raw.data).map_err(serde::de::Error::custom)?
                };
                Ok(ChildMessage::Tree { content: payload.content })
            }
            other => Err(serde::de::Error::custom(format!(
                "unknown message type {other}"
            ))),
        }
    }
}

impl fmt::Display for AlignValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CallbackData, ChildMessage, IconSource, PanelMessage, StatusData, StatusItem,
        TreeNode,
    };

    #[test]
    fn child_status_message_supports_name_and_path_icons() {
        let message = serde_json::from_value::<ChildMessage>(serde_json::json!({
            "type": "status",
            "data": {
                "items": [
                    {
                        "id": "weather",
                        "icon": {"type": "name", "value": "weather-clear-symbolic"},
                        "text": "21C"
                    },
                    {
                        "id": "avatar",
                        "icon": {"type": "path", "value": "/tmp/avatar.png"}
                    }
                ]
            }
        }))
        .expect("status message should parse");

        assert_eq!(
            message,
            ChildMessage::Status(StatusData {
                items: vec![
                    StatusItem {
                        id: Some("weather".into()),
                        icon: Some(IconSource::Name("weather-clear-symbolic".into())),
                        text: Some("21C".into()),
                    },
                    StatusItem {
                        id: Some("avatar".into()),
                        icon: Some(IconSource::Path("/tmp/avatar.png".into())),
                        text: None,
                    },
                ],
            })
        );
    }

    #[test]
    fn child_tree_message_parses_hero_node_as_content() {
        let message = serde_json::from_value::<ChildMessage>(serde_json::json!({
            "type": "tree",
            "data": {
                "content": {
                    "type": "hero",
                    "data": {
                        "title": "Weather",
                        "subtitle": "Sunny and 21C",
                        "icon": {"type": "name", "value": "weather-clear-symbolic"}
                    }
                }
            }
        }))
        .expect("tree message with hero node should parse");

        let ChildMessage::Tree { content } = message else {
            panic!("expected tree message");
        };
        assert!(matches!(content, Some(TreeNode::Hero(_))));
    }

    #[test]
    fn child_tree_message_clears_content_on_null_data() {
        let message = serde_json::from_value::<ChildMessage>(serde_json::json!({
            "type": "tree",
            "data": null
        }))
        .expect("tree clear should parse");

        assert_eq!(message, ChildMessage::Tree { content: None });
    }

    #[test]
    fn child_tree_message_accepts_box_with_hero_child() {
        let message = serde_json::from_value::<ChildMessage>(serde_json::json!({
            "type": "tree",
            "data": {
                "content": {
                    "type": "box",
                    "data": {
                        "orientation": "vertical",
                        "spacing": 8,
                        "children": [
                            {"type": "hero", "data": {"title": "Stats", "subtitle": "All good"}},
                            {"type": "label", "data": {"text": "Connected"}}
                        ]
                    }
                }
            }
        }))
        .expect("tree with hero child should parse");

        let ChildMessage::Tree { content } = message else {
            panic!("expected tree message");
        };
        assert!(matches!(content, Some(TreeNode::Box(_))));
    }

    #[test]
    fn callback_messages_serialize_uniform_event_shape() {
        let message = PanelMessage::Callback(CallbackData {
            id: "password_input".into(),
            event: "input".into(),
            text: Some("secret".into()),
            ..CallbackData::default()
        });

        let value = serde_json::to_value(message).expect("callback should serialize");
        assert_eq!(
            value,
            serde_json::json!({
                "type": "callback",
                "data": {
                    "id": "password_input",
                    "event": "input",
                    "text": "secret"
                }
            })
        );
    }
}
