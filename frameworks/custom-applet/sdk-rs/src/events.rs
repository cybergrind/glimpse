use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct InitEvent {
    pub instance: String,
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallbackEvent {
    Click(ClickEvent),
    Scroll(ScrollEvent),
    Input(InputEvent),
    Change(ChangeEvent),
    Toggle(ToggleEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClickEvent {
    pub id: String,
    pub button: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScrollEvent {
    pub id: String,
    pub delta_y: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputEvent {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangeEvent {
    pub id: String,
    pub value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleEvent {
    pub id: String,
    pub value: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct IncomingMessage {
    pub kind: String,
    pub data: Value,
}

#[derive(Debug, Deserialize)]
struct InitPayload {
    instance: String,
    #[serde(default)]
    options: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct CallbackPayload {
    id: String,
    #[serde(rename = "type")]
    event: String,
    #[serde(default)]
    button: Option<String>,
    #[serde(default)]
    delta_y: Option<f64>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    active: Option<bool>,
}

pub(crate) fn parse_incoming_line(line: &str) -> serde_json::Result<IncomingMessage> {
    let Some((kind, data)) = line.trim().split_once(char::is_whitespace) else {
        return Err(serde::de::Error::custom("missing command payload"));
    };
    Ok(IncomingMessage {
        kind: kind.into(),
        data: serde_json::from_str(data.trim_start())?,
    })
}

pub fn parse_init_event(data: Value) -> serde_json::Result<InitEvent> {
    let payload: InitPayload = serde_json::from_value(data)?;
    Ok(InitEvent {
        instance: payload.instance,
        options: payload.options,
    })
}

pub fn parse_callback_event(data: Value) -> serde_json::Result<CallbackEvent> {
    let payload: CallbackPayload = serde_json::from_value(data)?;
    let event = match payload.event.as_str() {
        "click" => CallbackEvent::Click(ClickEvent {
            id: payload.id,
            button: payload.button,
        }),
        "scroll" => CallbackEvent::Scroll(ScrollEvent {
            id: payload.id,
            delta_y: payload.delta_y,
        }),
        "input" => CallbackEvent::Input(InputEvent {
            id: payload.id,
            text: payload.text.unwrap_or_default(),
        }),
        "toggle" => CallbackEvent::Toggle(ToggleEvent {
            id: payload.id,
            value: payload
                .active
                .or_else(|| payload.value.and_then(|v| v.as_bool()))
                .unwrap_or(false),
        }),
        _ => CallbackEvent::Change(ChangeEvent {
            id: payload.id,
            value: payload.value,
        }),
    };
    Ok(event)
}
