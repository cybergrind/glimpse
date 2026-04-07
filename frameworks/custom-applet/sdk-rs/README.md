# Glimpse Applet Rust SDK

Small async framework for building Glimpse `exec` applets without touching stdio or raw JSON.

## Goals

- typed protocol models
- typed widget builders
- async runtime
- trait-based applet API
- state-driven rendering via `set_state(...)`
- single `render()` method returning all panel state

## Example

```rust
use async_trait::async_trait;
use glimpse_custom_applet_sdk::{
    run, Applet, AppletResult, BoxNode, Button, Hero, Icon, Label, RenderResult, StateStore,
    StatusItem, TreeNode,
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
            hero: Some(Hero::new("Counter", format!("Value: {}", self.state().count))),
            tree: Some(TreeNode::from(BoxNode::vertical(vec![
                TreeNode::from(Label::new(format!("Count = {}", self.state().count))),
                TreeNode::from(Button::new("increment").label("Increment")),
            ]))),
        })
    }
}
```
