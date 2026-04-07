import {
  Applet,
  Box,
  Button,
  Hero,
  Icon,
  Label,
  RenderResult,
  StatusItem,
} from "../src/index.js";

interface CounterState {
  count: number;
}

class CounterApplet extends Applet<CounterState> {
  protected initialState(): CounterState {
    return { count: 0 };
  }

  constructor() {
    super();
    this.onClick("increment", async () => {
      await this.setState({ count: this.state.count + 1 });
    });
  }

  protected async render(): Promise<RenderResult> {
    return new RenderResult({
      status: [
        new StatusItem({
          id: "counter",
          icon: Icon.name("view-refresh-symbolic"),
          text: String(this.state.count),
        }),
      ],
      hero: new Hero({
        icon: Icon.name("view-refresh-symbolic"),
        title: "Counter",
        subtitle: `Value: ${this.state.count}`,
      }),
      tree: Box.vertical([
        new Label(`Count = ${this.state.count}`),
        new Button({ id: "increment", label: "Increment" }),
      ], 8),
    });
  }
}

void new CounterApplet().run();
