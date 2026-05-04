import { createInterface } from "node:readline";

import {
  type CallbackEvent,
  type ChangeEvent,
  type ClickEvent,
  type InitEvent,
  type InputEvent,
  type ScrollEvent,
  type ToggleEvent,
  parseCallbackEvent,
  parseInitEvent,
} from "./events.js";
import { StatusItem } from "./protocol.js";
import { type TreeNode } from "./widgets.js";

type Handler<EventT> = (event: EventT) => void | Promise<void>;

interface OutgoingMessage {
  command: string;
  data: unknown;
  line: string;
}

export class RenderResult {
  constructor(
    public readonly options: {
      status?: StatusItem[];
      tree?: TreeNode | null;
    } = {},
  ) {}

  get status(): StatusItem[] {
    return this.options.status ?? [];
  }

  get tree(): TreeNode | null {
    return this.options.tree ?? null;
  }
}

export abstract class Applet<State extends object> {
  state: State;

  private readonly handlerMap = new Map<string, Handler<CallbackEvent>>();
  private readonly outgoing: OutgoingMessage[] = [];
  private flushPromise: Promise<void> | null = null;
  private renderQueued = false;
  private lastStatus: unknown[] | null = null;
  private lastTree: Record<string, unknown> | null = null;

  protected constructor() {
    this.state = this.initialState();
  }

  protected abstract initialState(): State;

  protected async onStart(): Promise<void> {}

  protected async onInit(_event: InitEvent): Promise<void> {}

  protected async onCallback(_event: CallbackEvent): Promise<void> {}

  protected async render(): Promise<RenderResult> {
    return new RenderResult();
  }

  async setState(patch: Partial<State>): Promise<void> {
    this.state = { ...this.state, ...patch };
    await this.scheduleRender();
  }

  onClick(id: string, handler: Handler<ClickEvent>): void {
    this.register("click", id, handler as Handler<CallbackEvent>);
  }

  onScroll(id: string, handler: Handler<ScrollEvent>): void {
    this.register("scroll", id, handler as Handler<CallbackEvent>);
  }

  onInput(id: string, handler: Handler<InputEvent>): void {
    this.register("input", id, handler as Handler<CallbackEvent>);
  }

  onChange(id: string, handler: Handler<ChangeEvent>): void {
    this.register("change", id, handler as Handler<CallbackEvent>);
  }

  onToggle(id: string, handler: Handler<ToggleEvent>): void {
    this.register("toggle", id, handler as Handler<CallbackEvent>);
  }

  async run(): Promise<void> {
    await this.onStart();
    await this.scheduleRender();

    const rl = createInterface({
      input: process.stdin,
      crlfDelay: Infinity,
    });

    for await (const line of rl) {
      if (!line) {
        continue;
      }
      const raw = parseLine(line);
      if (raw === null) {
        continue;
      }
      const data = raw.data as Record<string, unknown>;
      await this.handleIncoming(raw.command, data);
    }
  }

  protected async drainOutgoingForTest(): Promise<OutgoingMessage[]> {
    await this.scheduleRender();
    const drained = [...this.outgoing];
    this.outgoing.length = 0;
    return drained;
  }

  private register(event: string, id: string, handler: Handler<CallbackEvent>): void {
    this.handlerMap.set(`${event}:${id}`, handler);
  }

  private async dispatchCallback(event: CallbackEvent): Promise<void> {
    const handler = this.handlerMap.get(`${event.event}:${event.id}`);
    if (handler !== undefined) {
      await handler(event);
      return;
    }
    await this.onCallback(event);
  }

  private async handleIncoming(type: string, data: Record<string, unknown>): Promise<void> {
    if (type === "init") {
      await this.onInit(parseInitEvent(data));
      await this.scheduleRender();
      return;
    }
    if (type === "event") {
      await this.dispatchCallback(parseCallbackEvent(data));
    }
  }

  private async scheduleRender(): Promise<void> {
    this.renderQueued = true;
    if (this.flushPromise === null) {
      this.flushPromise = Promise.resolve().then(async () => {
        try {
          while (this.renderQueued) {
            this.renderQueued = false;
            await this.flushRender();
          }
        } finally {
          this.flushPromise = null;
        }
      });
    }
    await this.flushPromise;
  }

  private async flushRender(): Promise<void> {
    const rendered = await this.render();
    const status = rendered.status.map((item) => item.toProtocol());
    const tree = { root: rendered.tree?.toProtocol() ?? null };

    if (!deepEqual(status, this.lastStatus)) {
      this.lastStatus = status;
      this.emit("status", { items: status });
    }
    if (!deepEqual(tree, this.lastTree)) {
      this.lastTree = tree;
      this.emit("popover", tree);
    }
  }

  private emit(command: string, data: unknown): void {
    const line = `${command} ${JSON.stringify(data)}`;
    this.outgoing.push({ command, data, line });
    process.stdout.write(`${line}\n`);
  }
}

function parseLine(line: string): { command: string; data: unknown } | null {
  const trimmed = line.trim();
  if (trimmed === "") {
    return null;
  }
  const split = trimmed.search(/\s/);
  if (split < 0) {
    throw new Error("missing command payload");
  }
  return {
    command: trimmed.slice(0, split),
    data: JSON.parse(trimmed.slice(split).trimStart()),
  };
}

function deepEqual(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}
