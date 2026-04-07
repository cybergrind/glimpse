import test from "node:test";
import assert from "node:assert/strict";

import {
  Applet,
  Box,
  Button,
  Dropdown,
  DropdownItem,
  Hero,
  Icon,
  type InputEvent,
  Label,
  RenderResult,
  StatusItem,
  parseCallbackEvent,
} from "../src/index.js";

interface DemoState {
  version: string;
  clicks: number;
}

class DemoApplet extends Applet<DemoState> {
  protected initialState(): DemoState {
    return { version: "v1", clicks: 0 };
  }

  constructor() {
    super();
    this.onInput("version", async (event: InputEvent) => {
      await this.setState({ version: event.text });
    });
    this.onClick("submit", async () => {
      await this.setState({ clicks: this.state.clicks + 1 });
    });
  }

  protected async render(): Promise<RenderResult> {
    return new RenderResult({
      status: [
        new StatusItem({
          id: "demo",
          icon: Icon.name("demo-symbolic"),
          text: this.state.version,
        }),
      ],
      hero: new Hero({
        title: "Demo",
        subtitle: this.state.version,
      }),
      tree: Box.vertical([
        new Label(this.state.version),
        new Button({ id: "submit", label: "Submit" }),
      ]),
    });
  }

  async drain(): Promise<unknown[]> {
    return this.drainOutgoingForTest();
  }
}

test("setState updates state and emits protocol messages", async () => {
  const writes: string[] = [];
  const originalWrite = process.stdout.write.bind(process.stdout);
  process.stdout.write = ((chunk: string | Uint8Array) => {
    writes.push(typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8"));
    return true;
  }) as typeof process.stdout.write;

  try {
    const applet = new DemoApplet();
    await applet.setState({ version: "v2" });
    const drained = await applet.drain();
    const types = drained.map((message: any) => message.type);
    assert.deepEqual(types, ["status", "hero", "tree"]);
    assert.equal((drained[0] as any).data.items[0].text, "v2");
    assert.equal(writes.length, 3);
  } finally {
    process.stdout.write = originalWrite;
  }
});

test("parseCallbackEvent returns typed input event", () => {
  const event = parseCallbackEvent({ id: "version", event: "input", text: "abc" });
  assert.equal(event.event, "input");
  if (event.event !== "input") {
    throw new Error("expected input event");
  }
  assert.equal(event.text, "abc");
});

test("dropdown serializes items", () => {
  const node = new Dropdown({
    id: "env",
    items: [new DropdownItem("prod", "Production")],
    selected: 0,
  });
  const payload = node.toProtocol();
  assert.equal(payload.type, "dropdown");
  assert.equal((payload.data as any).items[0].id, "prod");
});

test("RenderResult defaults to empty status and null hero/tree", () => {
  const result = new RenderResult();
  assert.deepEqual(result.status, []);
  assert.equal(result.hero, null);
  assert.equal(result.tree, null);
});
