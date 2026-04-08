from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(slots=True)
class InitEvent:
    instance: str
    options: dict[str, Any]


@dataclass(slots=True)
class CallbackEvent:
    id: str
    event: str


@dataclass(slots=True)
class ClickEvent(CallbackEvent):
    button: str | None = None


@dataclass(slots=True)
class ScrollEvent(CallbackEvent):
    delta_y: float | None = None


@dataclass(slots=True)
class InputEvent(CallbackEvent):
    text: str = ""


@dataclass(slots=True)
class ChangeEvent(CallbackEvent):
    value: Any = None


@dataclass(slots=True)
class ToggleEvent(CallbackEvent):
    value: bool = False


def parse_init_event(payload: dict[str, Any]) -> InitEvent:
    return InitEvent(
        instance=str(payload.get("instance", "")),
        options=payload.get("options") or {},
    )


def parse_callback_event(payload: dict[str, Any]) -> CallbackEvent:
    event_type = str(payload.get("event", ""))
    callback_id = str(payload.get("id", ""))
    if event_type == "click":
        return ClickEvent(id=callback_id, event=event_type, button=payload.get("button"))
    if event_type == "scroll":
        return ScrollEvent(id=callback_id, event=event_type, delta_y=payload.get("delta_y"))
    if event_type == "input":
        return InputEvent(id=callback_id, event=event_type, text=str(payload.get("text", "")))
    if event_type == "toggle":
        return ToggleEvent(id=callback_id, event=event_type, value=bool(payload.get("value", False)))
    return ChangeEvent(id=callback_id, event=event_type, value=payload.get("value"))
