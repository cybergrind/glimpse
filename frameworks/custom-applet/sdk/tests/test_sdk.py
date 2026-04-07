from __future__ import annotations

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
    Hero,
    Icon,
    InputEvent,
    Label,
    RenderResult,
    StatusItem,
    click,
    input,
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
                StatusItem(id="demo", icon=Icon.name("demo-symbolic"), text=self.state.version)
            ],
            hero=Hero(title="Demo", subtitle=self.state.version),
            tree=Box.vertical([Label(self.state.version), Button(id="submit", label="Submit")]),
        )

    @input("version")
    async def handle_version(self, event: InputEvent) -> None:
        await self.set_state(version=event.text)

    @click("submit")
    async def handle_submit(self, _event) -> None:
        await self.set_state(clicks=self.state.clicks + 1)


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
        hero = await applet._outgoing.get()
        tree = await applet._outgoing.get()
        self.assertEqual(status["type"], "status")
        self.assertEqual(hero["type"], "hero")
        self.assertEqual(tree["type"], "tree")

    async def test_render_result_defaults_allow_partial_updates(self) -> None:
        result = RenderResult()
        self.assertEqual(result.status, [])
        self.assertIsNone(result.hero)
        self.assertIsNone(result.tree)

    def test_parse_callback_event_returns_typed_variant(self) -> None:
        event = parse_callback_event({"id": "version", "event": "input", "text": "abc"})
        self.assertIsInstance(event, InputEvent)
        self.assertEqual(event.text, "abc")

    def test_dropdown_serializes_items(self) -> None:
        node = Dropdown(id="env", items=[DropdownItem(id="prod", label="Production")], selected=0)
        payload = node.to_protocol()
        self.assertEqual(payload["type"], "dropdown")
        self.assertEqual(payload["data"]["items"][0]["id"], "prod")


if __name__ == "__main__":
    unittest.main()
