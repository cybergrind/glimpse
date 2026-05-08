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
  type InitEvent,
  Label,
  RenderResult,
  Row,
  StatusItem,
  StatusMenuItem,
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
    this.onClick("submit", async () => {
      await this.setState({ clicks: this.state.clicks + 1, version: "v2" });
    });
  }

  protected async render(): Promise<RenderResult> {
    return new RenderResult({
      status: [
        new StatusItem({
          id: "demo",
          icon: Icon.name("demo-symbolic"),
          label: this.state.version,
        }),
      ],
      tree: Box.vertical([
        new Hero({ title: "Demo", subtitle: this.state.version }),
        new Label(this.state.version),
        new Button({ id: "submit", label: "Submit" }),
      ]),
    });
  }

  protected async onInit(event: InitEvent): Promise<void> {
    this.state.version = event.instance;
  }

  async drain(): Promise<unknown[]> {
    return this.drainOutgoingForTest();
  }

  async initForTest(instance: string): Promise<void> {
    await (this as any).handleIncoming("init", { instance, options: {} });
  }

  async eventForTest(payload: Record<string, unknown>): Promise<void> {
    await (this as any).handleIncoming("event", payload);
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
    const commands = drained.map((message: any) => message.command);
    assert.deepEqual(commands, ["status", "popover"]);
    assert.equal((drained[0] as any).data.items[0].label, "v2");
    assert.equal((drained[0] as any).line, 'status {"items":[{"id":"demo","icon":{"name":"demo-symbolic"},"label":"v2"}]}');
    assert.equal(writes.length, 2);
  } finally {
    process.stdout.write = originalWrite;
  }
});

test("parseCallbackEvent returns typed click event", () => {
  const event = parseCallbackEvent({ id: "submit", type: "click", button: "left" });
  assert.equal(event.event, "click");
  if (event.event !== "click") {
    throw new Error("expected click event");
  }
  assert.equal(event.button, "left");
});

test("parseCallbackEvent returns typed popover event", () => {
  const event = parseCallbackEvent({ id: "popover", type: "open", source: "popover" });
  assert.equal(event.event, "open");
  if (event.event !== "open") {
    throw new Error("expected open event");
  }
  assert.equal(event.open, true);
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

test("row serializes as action_row", () => {
  const payload = new Row({ id: "open", title: "Open" }).toProtocol();
  assert.equal(payload.type, "action_row");
});

test("status item serializes menu items", () => {
  const payload = new StatusItem({
    id: "github-workflows",
    label: "CI",
    menu: [
      new StatusMenuItem({ id: "refresh", label: "Refresh" }),
      new StatusMenuItem({ id: "open", label: "Open Actions", enabled: false }),
    ],
  }).toProtocol();

  assert.equal((payload.menu as any[])[0].id, "refresh");
  assert.equal((payload.menu as any[])[0].label, "Refresh");
  assert.equal((payload.menu as any[])[1].enabled, false);
});

test("closed popover updates are dropped until opened", async () => {
  const applet = new DemoApplet();
  await applet.drain();
  await applet.setState({ version: "v2" });

  let drained = await applet.drain();
  assert.deepEqual(
    drained.map((message: any) => message.command),
    ["status"],
  );

  await applet.eventForTest({ id: "popover", type: "open", source: "popover" });
  drained = await applet.drain();
  assert.ok(drained.some((message: any) => message.command === "popover"));
});

test("init event rerenders changed state", async () => {
  const applet = new DemoApplet();
  await applet.drain();
  await applet.initForTest("v3");
  const drained = await applet.drain();
  assert.equal((drained[0] as any).data.items[0].label, "v3");
});

test("variant serializes as semantic protocol value", () => {
  const payload = new Label("Warning", { variant: "warning" }).toProtocol();
  assert.equal((payload.data as any).variant, "warning");
});

test("RenderResult defaults to empty status and null tree", () => {
  const result = new RenderResult();
  assert.deepEqual(result.status, []);
  assert.equal(result.tree, null);
});
