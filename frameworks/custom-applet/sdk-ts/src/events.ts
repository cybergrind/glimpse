export interface InitEvent {
  instance: string;
}

interface CallbackEventBase {
  id: string;
  event: string;
}

export interface ClickEvent extends CallbackEventBase {
  event: "click";
  button?: string;
}

export interface ScrollEvent extends CallbackEventBase {
  event: "scroll";
  delta_y?: number;
}

export interface InputEvent extends CallbackEventBase {
  event: "input";
  text: string;
}

export interface ChangeEvent extends CallbackEventBase {
  event: "change";
  value: unknown;
}

export interface ToggleEvent extends CallbackEventBase {
  event: "toggle";
  value: boolean;
}

export type CallbackEvent =
  | ClickEvent
  | ScrollEvent
  | InputEvent
  | ChangeEvent
  | ToggleEvent;

export function parseInitEvent(payload: Record<string, unknown>): InitEvent {
  return {
    instance: String(payload.instance ?? ""),
  };
}

export function parseCallbackEvent(payload: Record<string, unknown>): CallbackEvent {
  const event = String(payload.event ?? "");
  const id = String(payload.id ?? "");
  if (event === "click") {
    return { id, event, button: payload.button === undefined ? undefined : String(payload.button) };
  }
  if (event === "scroll") {
    return {
      id,
      event,
      delta_y: typeof payload.delta_y === "number" ? payload.delta_y : undefined,
    };
  }
  if (event === "input") {
    return { id, event, text: String(payload.text ?? "") };
  }
  if (event === "toggle") {
    return { id, event, value: Boolean(payload.value) };
  }
  return { id, event: "change", value: payload.value };
}
