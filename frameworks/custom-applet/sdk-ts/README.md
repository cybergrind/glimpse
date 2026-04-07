# Glimpse Applet TypeScript SDK

Small async framework for building Glimpse `exec` applets without touching stdio or raw JSON.

## Goals

- typed protocol models
- typed widget builders
- async runtime
- explicit typed handler registration
- state-driven rendering via `await this.setState(...)`
- single `render()` method that returns all panel state

## Example

```ts
import {
  Applet,
  Box,
  Button,
  Hero,
  Icon,
  Label,
  RenderResult,
  StatusItem,
} from "./src/index.js";

interface DeployState {
  version: string;
  status: string;
}

class DeployApplet extends Applet<DeployState> {
  protected initialState(): DeployState {
    return { version: "2026.04.07", status: "Ready" };
  }

  constructor() {
    super();
    this.onClick("deploy_now", async () => {
      await this.setState({ status: "Deploying" });
    });
  }

  protected async render(): Promise<RenderResult> {
    return new RenderResult({
      status: [
        new StatusItem({
          id: "deploy",
          icon: Icon.name("software-update-available-symbolic"),
          text: this.state.status,
        }),
      ],
      hero: new Hero({
        icon: Icon.name("software-update-available-symbolic"),
        title: "Deploy",
        subtitle: this.state.version,
      }),
      tree: Box.vertical([
        new Label("Version"),
        new Button({ id: "deploy_now", label: "Deploy now" }),
      ]),
    });
  }
}

await new DeployApplet().run();
```

## Handler Registration

Use explicit registration helpers instead of decorators:

- `this.onClick(id, handler)`
- `this.onScroll(id, handler)`
- `this.onInput(id, handler)`
- `this.onChange(id, handler)`
- `this.onToggle(id, handler)`

The SDK owns the JSON-lines transport and writes `status`, `hero`, and `tree` messages derived from `render()`.
