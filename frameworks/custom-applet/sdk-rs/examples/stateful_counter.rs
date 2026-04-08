use async_trait::async_trait;
use glimpse_custom_applet_sdk::{
    Applet, AppletResult, BoxNode, Button, Hero, Icon, Label, RenderResult, StateStore,
    StatusItem, TreeNode, run,
};

#[derive(Debug, Clone, Default)]
struct CounterState {
    count: u32,
}

struct CounterApplet {
    store: StateStore<CounterState>,
}

#[async_trait]
impl Applet for CounterApplet {
    type State = CounterState;

    fn store(&self) -> &StateStore<Self::State> {
        &self.store
    }

    fn store_mut(&mut self) -> &mut StateStore<Self::State> {
        &mut self.store
    }

    async fn render(&self) -> AppletResult<RenderResult> {
        Ok(RenderResult {
            status: vec![StatusItem::new("counter")
                .icon(Icon::name("view-refresh-symbolic"))
                .text(self.state().count.to_string())],
            tree: Some(TreeNode::from(
                BoxNode::vertical(vec![
                    TreeNode::from(
                        Hero::new("Counter", format!("Value: {}", self.state().count))
                            .icon(Icon::name("view-refresh-symbolic")),
                    ),
                    TreeNode::from(Label::new(format!("Count = {}", self.state().count))),
                    TreeNode::from(Button::new("increment").label("Increment")),
                ])
                .spacing(8),
            )),
        })
    }

    async fn on_callback(
        &mut self,
        event: glimpse_custom_applet_sdk::CallbackEvent,
    ) -> AppletResult<()> {
        if let glimpse_custom_applet_sdk::CallbackEvent::Click(click) = event {
            if click.id == "increment" {
                self.set_state(|state| state.count += 1);
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> AppletResult<()> {
    run(CounterApplet {
        store: StateStore::new(CounterState::default()),
    })
    .await
}
