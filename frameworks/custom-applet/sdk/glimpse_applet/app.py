from __future__ import annotations

import asyncio
import json
import sys
from dataclasses import dataclass, is_dataclass
from typing import Any, Generic, TypeVar

from .events import CallbackEvent, InitEvent, parse_callback_event, parse_init_event
from .protocol import Hero, StatusItem
from .widgets import TreeNode

StateT = TypeVar("StateT", bound="AppletState")


@dataclass(slots=True)
class AppletState:
    pass


@dataclass(slots=True)
class RenderResult:
    status: list[StatusItem] | None = None
    hero: Hero | None = None
    tree: TreeNode | None = None

    def __post_init__(self) -> None:
        if self.status is None:
            self.status = []


class Applet(Generic[StateT]):
    def __init__(self) -> None:
        self.state: StateT = self.initial_state()
        self._incoming: asyncio.Queue[InitEvent | CallbackEvent] = asyncio.Queue()
        self._outgoing: asyncio.Queue[dict[str, Any]] = asyncio.Queue()
        self._handler_map = self._collect_handlers()
        self._render_task: asyncio.Task[None] | None = None
        self._last_status: list[dict[str, Any]] | None = None
        self._last_hero: dict[str, Any] | None = None
        self._last_tree: dict[str, Any] | None = None

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

    def _schedule_render(self) -> None:
        if self._render_task is None or self._render_task.done():
            self._render_task = asyncio.create_task(self._flush_render())

    async def _flush_render(self) -> None:
        await asyncio.sleep(0)
        rendered = await self.render()
        status = [item.to_protocol() for item in rendered.status]
        hero_obj = rendered.hero
        hero = None if hero_obj is None else hero_obj.to_protocol()
        tree_obj = rendered.tree
        tree = None if tree_obj is None else {"content": tree_obj.to_protocol()}

        if status != self._last_status:
            self._last_status = status
            await self._outgoing.put({"type": "status", "data": {"items": status}})
        if hero != self._last_hero:
            self._last_hero = hero
            await self._outgoing.put({"type": "hero", "data": hero})
        if tree != self._last_tree:
            self._last_tree = tree
            await self._outgoing.put({"type": "tree", "data": tree})

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
            raw = json.loads(line)
            message_type = raw.get("type")
            data = raw.get("data", {})
            if message_type == "init":
                await self._incoming.put(parse_init_event(data))
            elif message_type == "callback":
                await self._incoming.put(parse_callback_event(data))

    async def _writer_loop(self) -> None:
        while True:
            payload = await self._outgoing.get()
            sys.stdout.write(json.dumps(payload) + "\n")
            sys.stdout.flush()

    async def _event_loop(self) -> None:
        await self.on_start()
        self._schedule_render()
        while True:
            event = await self._incoming.get()
            if isinstance(event, InitEvent):
                await self.on_init(event)
            else:
                await self._dispatch_callback(event)

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
