from __future__ import annotations

import asyncio
import json
import sys
from dataclasses import dataclass, is_dataclass
from typing import Any, Generic, TypeVar

from .events import CallbackEvent, InitEvent, PopoverEvent, parse_callback_event, parse_init_event
from .protocol import StatusItem
from .widgets import TreeNode

StateT = TypeVar("StateT", bound="AppletState")


@dataclass(slots=True)
class AppletState:
    pass


@dataclass(slots=True)
class RenderResult:
    status: list[StatusItem] | None = None
    tree: TreeNode | None = None

    def __post_init__(self) -> None:
        if self.status is None:
            self.status = []


class Applet(Generic[StateT]):
    def __init__(self) -> None:
        self.state: StateT = self.initial_state()
        self._incoming: asyncio.Queue[InitEvent | CallbackEvent] = asyncio.Queue()
        self._outgoing: asyncio.Queue[tuple[str, dict[str, Any]]] = asyncio.Queue()
        self._handler_map = self._collect_handlers()
        self._render_task: asyncio.Task[None] | None = None
        self._render_requested = False
        self._last_status: list[dict[str, Any]] | None = None
        self._last_tree: dict[str, Any] | None = None
        self._popover_open = False

    def initial_state(self) -> StateT:
        raise NotImplementedError

    async def on_start(self) -> None:
        return None

    async def on_init(self, _event: InitEvent) -> None:
        return None

    async def on_callback(self, _event: CallbackEvent) -> None:
        return None

    async def render(self) -> RenderResult:
        return RenderResult()

    async def set_state(self, **kwargs: Any) -> None:
        for key, value in kwargs.items():
            if not hasattr(self.state, key):
                raise AttributeError(f"Unknown state field: {key}")
            setattr(self.state, key, value)
        self._schedule_render()
        await asyncio.sleep(0)

    def is_popover_open(self) -> bool:
        return self._popover_open

    def _schedule_render(self) -> None:
        self._render_requested = True
        if self._render_task is None or self._render_task.done():
            self._render_task = asyncio.create_task(self._flush_render())

    async def _flush_render(self) -> None:
        await asyncio.sleep(0)
        while self._render_requested:
            self._render_requested = False
            rendered = await self.render()
            status = [item.to_protocol() for item in rendered.status]
            content = None if rendered.tree is None else rendered.tree.to_protocol()
            tree = {"root": content}

            if status != self._last_status:
                self._last_status = status
                await self._outgoing.put(("status", {"items": status}))
            publish_popover = self._popover_open or self._last_tree is None or content is None
            if publish_popover and tree != self._last_tree:
                self._last_tree = tree
                await self._outgoing.put(("popover", tree))

    def _collect_handlers(self) -> dict[tuple[str, str], Any]:
        handlers: dict[tuple[str, str], Any] = {}
        for name in dir(self):
            value = getattr(self, name)
            handler_meta = getattr(value, "__glimpse_handler__", None)
            if handler_meta is not None:
                handlers[handler_meta] = value
        return handlers

    async def _dispatch_callback(self, event: CallbackEvent) -> None:
        handler = self._handler_map.get((event.event, event.id))
        if handler is not None:
            await handler(event)
        else:
            await self.on_callback(event)

    async def _reader_loop(self) -> None:
        while True:
            line = await asyncio.to_thread(sys.stdin.readline)
            if line == "":
                break
            parsed = _parse_line(line)
            if parsed is None:
                continue
            message_type, data = parsed
            if message_type == "init":
                await self._incoming.put(parse_init_event(data))
            elif message_type == "event":
                await self._incoming.put(parse_callback_event(data))

    async def _writer_loop(self) -> None:
        while True:
            command, payload = await self._outgoing.get()
            sys.stdout.write(f"{command} {json.dumps(payload, separators=(',', ':'))}\n")
            sys.stdout.flush()

    async def _event_loop(self) -> None:
        await self.on_start()
        self._schedule_render()
        while True:
            event = await self._incoming.get()
            if isinstance(event, InitEvent):
                await self.on_init(event)
                self._schedule_render()
                await asyncio.sleep(0)
            else:
                if isinstance(event, PopoverEvent):
                    self._popover_open = event.open
                await self._dispatch_callback(event)
                self._schedule_render()
                await asyncio.sleep(0)

    async def _run(self) -> None:
        if not is_dataclass(self.state):
            raise TypeError("Applet state must be a dataclass instance")
        writer = asyncio.create_task(self._writer_loop())
        reader = asyncio.create_task(self._reader_loop())
        try:
            await self._event_loop()
        finally:
            reader.cancel()
            writer.cancel()

    def run(self) -> None:
        asyncio.run(self._run())


def _parse_line(line: str) -> tuple[str, dict[str, Any]] | None:
    stripped = line.strip()
    if not stripped:
        return None
    command, _, payload = stripped.partition(" ")
    if not payload:
        raise ValueError("missing command payload")
    data = json.loads(payload)
    if not isinstance(data, dict):
        raise ValueError("command payload must be an object")
    return command, data
