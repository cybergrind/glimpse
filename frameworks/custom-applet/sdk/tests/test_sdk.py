from __future__ import annotations

import asyncio
import contextlib
import unittest
from dataclasses import dataclass

from glimpse_applet import (
    Applet,
    AppletState,
    Box,
    Button,
    ChangeEvent,
    Dropdown,
    DropdownItem,
    Icon,
    InitEvent,
    Item,
    Label,
    PopoverEvent,
    RenderResult,
    Row,
    StatusItem,
    MenuItem,
    Variant,
    click,
)
from glimpse_applet.events import parse_callback_event


@dataclass
class DemoState(AppletState):
    version: str = "v1"
    clicks: int = 0


class DemoApplet(Applet[DemoState]):
    def initial_state(self) -> DemoState:
        return DemoState()

    async def render(self) -> RenderResult:
        return RenderResult(
            status=[
                StatusItem(id="demo", icon=Icon.name("demo-symbolic"), label=self.state.version)
            ],
            tree=Box.vertical([Label(text=self.state.version), Button(id="submit", label="Submit")]),
        )

    @click("submit")
    async def handle_submit(self, _event) -> None:
        await self.set_state(clicks=self.state.clicks + 1, version="v2")


class InitApplet(DemoApplet):
    async def on_init(self, event: InitEvent) -> None:
        self.state.version = event.instance


class GlimpseAppletTests(unittest.IsolatedAsyncioTestCase):
    async def test_set_state_updates_dataclass_fields(self) -> None:
        applet = DemoApplet()
        await applet.set_state(version="v2")
        self.assertEqual(applet.state.version, "v2")

    async def test_render_flush_emits_protocol_messages(self) -> None:
        applet = DemoApplet()
        await applet.set_state(version="v2")
        await applet._flush_render()
        status = await applet._outgoing.get()
        tree = await applet._outgoing.get()
        self.assertEqual(status[0], "status")
        self.assertEqual(status[1]["items"][0]["label"], "v2")
        self.assertEqual(tree[0], "popover")
        self.assertIn("root", tree[1])

    async def test_render_result_defaults_allow_partial_updates(self) -> None:
        result = RenderResult()
        self.assertEqual(result.status, [])
        self.assertIsNone(result.tree)

    def test_parse_callback_event_returns_typed_variant(self) -> None:
        event = parse_callback_event({"id": "submit", "type": "click", "button": "left"})
        self.assertEqual(event.event, "click")
        self.assertEqual(getattr(event, "button"), "left")

    def test_parse_callback_event_returns_typed_popover_variant(self) -> None:
        event = parse_callback_event({"id": "popover", "type": "open", "source": "popover"})
        self.assertIsInstance(event, PopoverEvent)
        self.assertTrue(getattr(event, "open"))

    def test_dropdown_serializes_items(self) -> None:
        node = Dropdown(id="env", items=[DropdownItem(id="prod", label="Production")], selected=0)
        payload = node.to_protocol()
        self.assertEqual(payload["type"], "dropdown")
        self.assertEqual(payload["data"]["items"][0]["id"], "prod")

    def test_row_serializes_as_action_row(self) -> None:
        payload = Row(id="open", title="Open").to_protocol()
        self.assertEqual(payload["type"], "action_row")

    def test_variant_serializes_as_semantic_protocol_value(self) -> None:
        payload = Label(text="Warning", variant=Variant.WARNING).to_protocol()
        self.assertEqual(payload["data"]["variant"], "warning")

    def test_item_serializes_menu_items(self) -> None:
        payload = Item(
            id="run",
            label="Run",
            clickable=True,
            menu=[
                MenuItem(id="open", label="Open"),
                MenuItem(id="cancel", label="Cancel", enabled=False),
            ],
        ).to_protocol()

        self.assertEqual(payload["type"], "item")
        self.assertEqual(payload["data"]["menu"][0]["id"], "open")
        self.assertEqual(payload["data"]["menu"][1]["enabled"], False)

    async def test_init_event_rerenders_changed_state(self) -> None:
        applet = InitApplet()
        loop_task = asyncio.create_task(applet._event_loop())
        try:
            status = await applet._outgoing.get()
            await applet._outgoing.get()
            self.assertEqual(status[1]["items"][0]["label"], "v1")

            await applet._incoming.put(InitEvent(instance="v3", options={}))
            status = await applet._outgoing.get()
            self.assertEqual(status[1]["items"][0]["label"], "v3")
        finally:
            loop_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await loop_task

    async def test_closed_popover_updates_are_dropped_until_opened(self) -> None:
        applet = DemoApplet()
        applet._render_requested = True
        await applet._flush_render()
        await applet._outgoing.get()
        await applet._outgoing.get()

        await applet.set_state(version="v2")
        status = await applet._outgoing.get()
        self.assertEqual(status[0], "status")
        self.assertTrue(applet._outgoing.empty())

        applet._popover_open = True
        applet._render_requested = True
        await applet._flush_render()
        command, payload = await applet._outgoing.get()
        self.assertEqual(command, "popover")
        self.assertEqual(payload["root"]["data"]["children"][0]["data"]["text"], "v2")


if __name__ == "__main__":
    unittest.main()
