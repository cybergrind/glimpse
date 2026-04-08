use async_trait::async_trait;
use serde::Serialize;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    events::{CallbackEvent, IncomingMessage, InitEvent, parse_callback_event, parse_init_event},
    protocol::StatusItem,
    widgets::TreeNode,
};

pub type AppletError = Box<dyn std::error::Error + Send + Sync>;
pub type AppletResult<T> = Result<T, AppletError>;

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct RenderResult {
    #[serde(default)]
    pub status: Vec<StatusItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<TreeNode>,
}

#[derive(Debug, Clone)]
pub struct StateStore<State> {
    state: State,
}

impl<State> StateStore<State> {
    pub fn new(state: State) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut State {
        &mut self.state
    }

    pub fn set_state<F>(&mut self, patch: F)
    where
        F: FnOnce(&mut State),
    {
        patch(&mut self.state);
    }
}

#[async_trait]
pub trait Applet: Send {
    type State: Send + Sync + 'static;

    fn store(&self) -> &StateStore<Self::State>;
    fn store_mut(&mut self) -> &mut StateStore<Self::State>;

    async fn render(&self) -> AppletResult<RenderResult>;

    async fn on_start(&mut self) -> AppletResult<()> {
        Ok(())
    }

    async fn on_init(&mut self, _event: InitEvent) -> AppletResult<()> {
        Ok(())
    }

    async fn on_callback(&mut self, _event: CallbackEvent) -> AppletResult<()> {
        Ok(())
    }

    fn state(&self) -> &Self::State {
        self.store().state()
    }

    fn state_mut(&mut self) -> &mut Self::State {
        self.store_mut().state_mut()
    }

    fn set_state<F>(&mut self, patch: F)
    where
        F: FnOnce(&mut Self::State),
    {
        self.store_mut().set_state(patch);
    }
}

#[derive(Debug, Serialize, PartialEq)]
struct OutgoingMessage<T> {
    #[serde(rename = "type")]
    kind: &'static str,
    data: T,
}

#[derive(Debug, Serialize, PartialEq)]
struct TreePayload {
    content: Option<TreeNode>,
}

pub async fn run<A>(mut applet: A) -> AppletResult<()>
where
    A: Applet,
{
    let mut stdout = io::stdout();
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut last_render: Option<RenderResult> = None;

    applet.on_start().await?;
    let initial = applet.render().await?;
    flush_render(&mut stdout, &mut last_render, initial).await?;

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let incoming: IncomingMessage = serde_json::from_str(&line)?;
        match incoming.kind.as_str() {
            "init" => {
                applet.on_init(parse_init_event(incoming.data)?).await?;
            }
            "callback" => {
                applet
                    .on_callback(parse_callback_event(incoming.data)?)
                    .await?;
            }
            _ => continue,
        }

        let rendered = applet.render().await?;
        flush_render(&mut stdout, &mut last_render, rendered).await?;
    }

    Ok(())
}

async fn flush_render(
    stdout: &mut io::Stdout,
    previous: &mut Option<RenderResult>,
    next: RenderResult,
) -> AppletResult<()> {
    let changed = previous.as_ref();
    if changed.map(|prev| prev.status != next.status).unwrap_or(true) {
        write_message(
            stdout,
            &OutgoingMessage {
                kind: "status",
                data: serde_json::json!({ "items": next.status }),
            },
        )
        .await?;
    }
    if changed.map(|prev| prev.tree != next.tree).unwrap_or(true) {
        write_message(
            stdout,
            &OutgoingMessage {
                kind: "tree",
                data: TreePayload {
                    content: next.tree.clone(),
                },
            },
        )
        .await?;
    }

    *previous = Some(next);
    Ok(())
}

async fn write_message<T: Serialize>(stdout: &mut io::Stdout, message: &T) -> AppletResult<()> {
    let encoded = serde_json::to_vec(message)?;
    stdout.write_all(&encoded).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{BoxNode, Button, CallbackEvent, Icon, InputEvent, Label, StatusItem, TreeNode};

    struct DemoApplet {
        store: StateStore<DemoState>,
    }

    #[derive(Debug, Clone)]
    struct DemoState {
        version: String,
        clicks: u32,
    }

    #[async_trait]
    impl Applet for DemoApplet {
        type State = DemoState;

        fn store(&self) -> &StateStore<Self::State> {
            &self.store
        }

        fn store_mut(&mut self) -> &mut StateStore<Self::State> {
            &mut self.store
        }

        async fn render(&self) -> AppletResult<RenderResult> {
            Ok(RenderResult {
                status: vec![StatusItem::new("demo")
                    .icon(Icon::name("demo-symbolic"))
                    .text(self.state().version.clone())],
                tree: Some(TreeNode::from(BoxNode::vertical(vec![
                    TreeNode::from(crate::Hero::new("Demo", self.state().version.clone())),
                    TreeNode::from(Label::new(self.state().version.clone())),
                    TreeNode::from(Button::new("submit").label("Submit")),
                ]))),
            })
        }

        async fn on_callback(&mut self, event: CallbackEvent) -> AppletResult<()> {
            match event {
                CallbackEvent::Input(InputEvent { id, text }) if id == "version" => {
                    self.set_state(|state| state.version = text);
                }
                CallbackEvent::Click(click) if click.id == "submit" => {
                    self.set_state(|state| state.clicks += 1);
                }
                _ => {}
            }
            Ok(())
        }
    }

    #[test]
    fn render_result_defaults_are_empty() {
        let result = RenderResult::default();
        assert!(result.status.is_empty());
        assert!(result.tree.is_none());
    }

    #[test]
    fn parse_callback_event_returns_typed_input_variant() {
        let event = parse_callback_event(json!({
            "id": "version",
            "event": "input",
            "text": "abc"
        }))
        .expect("input event should parse");

        assert_eq!(
            event,
            CallbackEvent::Input(InputEvent {
                id: "version".into(),
                text: "abc".into(),
            })
        );
    }

    #[test]
    fn dropdown_like_tree_nodes_serialize() {
        let node = crate::Dropdown::new(
            "env",
            vec![crate::DropdownItem::new("prod", "Production")],
        );
        let payload = serde_json::to_value(TreeNode::from(node)).expect("tree should serialize");
        assert_eq!(payload["type"], "dropdown");
        assert_eq!(payload["data"]["items"][0]["id"], "prod");
    }

    #[tokio::test]
    async fn set_state_updates_rendered_status() {
        let mut applet = DemoApplet {
            store: StateStore::new(DemoState {
                version: "v1".into(),
                clicks: 0,
            }),
        };

        applet
            .on_callback(CallbackEvent::Input(InputEvent {
                id: "version".into(),
                text: "v2".into(),
            }))
            .await
            .expect("callback should update state");

        let rendered = applet.render().await.expect("render should succeed");
        assert_eq!(rendered.status[0].text.as_deref(), Some("v2"));
    }
}
