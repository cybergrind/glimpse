from __future__ import annotations

from dataclasses import dataclass, field
from enum import StrEnum
from typing import TypeAlias

from .protocol import Icon


class Align(StrEnum):
    FILL = "fill"
    START = "start"
    END = "end"
    CENTER = "center"
    BASELINE = "baseline"


class Orientation(StrEnum):
    HORIZONTAL = "horizontal"
    VERTICAL = "vertical"


@dataclass(slots=True)
class CommonProps:
    id: str | None = None
    visible: bool | None = None
    hexpand: bool | None = None
    vexpand: bool | None = None
    halign: Align | None = None
    valign: Align | None = None
    tooltip: str | None = None
    css_classes: list[str] = field(default_factory=list)

    def apply_common(self, payload: dict[str, object]) -> dict[str, object]:
        if self.id is not None:
            payload["id"] = self.id
        if self.visible is not None:
            payload["visible"] = self.visible
        if self.hexpand is not None:
            payload["hexpand"] = self.hexpand
        if self.vexpand is not None:
            payload["vexpand"] = self.vexpand
        if self.halign is not None:
            payload["halign"] = self.halign.value
        if self.valign is not None:
            payload["valign"] = self.valign.value
        if self.tooltip is not None:
            payload["tooltip"] = self.tooltip
        if self.css_classes:
            payload["css_classes"] = self.css_classes
        return payload


class Widget(CommonProps):
    widget_type: str = ""

    def to_protocol(self) -> dict[str, object]:
        raise NotImplementedError


@dataclass(slots=True)
class Label(Widget):
    text: str = ""
    wrap: bool = False
    xalign: float | None = None
    selectable: bool = False
    widget_type: str = "label"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"text": self.text})
        if self.wrap:
            payload["wrap"] = self.wrap
        if self.xalign is not None:
            payload["xalign"] = self.xalign
        if self.selectable:
            payload["selectable"] = self.selectable
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Image(Widget):
    icon: Icon | None = None
    pixel_size: int | None = None
    widget_type: str = "image"

    def to_protocol(self) -> dict[str, object]:
        if self.icon is None:
            raise ValueError("Image requires an icon")
        payload = self.apply_common({"icon": self.icon.to_protocol()})
        if self.pixel_size is not None:
            payload["pixel_size"] = self.pixel_size
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Button(Widget):
    label: str | None = None
    icon: Icon | None = None
    child: "TreeNode | None" = None
    widget_type: str = "button"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({})
        if self.label is not None:
            payload["label"] = self.label
        if self.icon is not None:
            payload["icon"] = self.icon.to_protocol()
        if self.child is not None:
            payload["child"] = self.child.to_protocol()
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Entry(Widget):
    text: str = ""
    placeholder: str | None = None
    widget_type: str = "entry"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"text": self.text})
        if self.placeholder is not None:
            payload["placeholder"] = self.placeholder
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Password(Entry):
    widget_type: str = "password"


@dataclass(slots=True)
class Switch(Widget):
    label: str | None = None
    active: bool = False
    widget_type: str = "switch"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"active": self.active})
        if self.label is not None:
            payload["label"] = self.label
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Scale(Widget):
    min: float = 0.0
    max: float = 1.0
    step: float = 0.1
    value: float = 0.0
    orientation: Orientation | None = None
    draw_value: bool = False
    widget_type: str = "scale"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "min": self.min,
                "max": self.max,
                "step": self.step,
                "value": self.value,
            }
        )
        if self.orientation is not None:
            payload["orientation"] = self.orientation.value
        if self.draw_value:
            payload["draw_value"] = self.draw_value
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Checkbox(Widget):
    label: str | None = None
    active: bool = False
    widget_type: str = "checkbox"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"active": self.active})
        if self.label is not None:
            payload["label"] = self.label
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class DropdownItem:
    id: str
    label: str

    def to_protocol(self) -> dict[str, str]:
        return {"id": self.id, "label": self.label}


@dataclass(slots=True)
class Dropdown(Widget):
    items: list[DropdownItem] = field(default_factory=list)
    selected: int | None = None
    widget_type: str = "dropdown"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {"items": [item.to_protocol() for item in self.items]}
        )
        if self.selected is not None:
            payload["selected"] = self.selected
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Separator(Widget):
    orientation: Orientation | None = None
    widget_type: str = "separator"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({})
        if self.orientation is not None:
            payload["orientation"] = self.orientation.value
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Scroll(Widget):
    child: "TreeNode | None" = None
    widget_type: str = "scroll"

    def to_protocol(self) -> dict[str, object]:
        if self.child is None:
            raise ValueError("Scroll requires a child")
        payload = self.apply_common({"child": self.child.to_protocol()})
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class GridChild:
    row: int
    column: int
    child: "TreeNode"
    width: int = 1
    height: int = 1

    def to_protocol(self) -> dict[str, object]:
        return {
            "row": self.row,
            "column": self.column,
            "width": self.width,
            "height": self.height,
            "child": self.child.to_protocol(),
        }


@dataclass(slots=True)
class Grid(Widget):
    children: list[GridChild] = field(default_factory=list)
    row_spacing: int = 0
    column_spacing: int = 0
    widget_type: str = "grid"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "row_spacing": self.row_spacing,
                "column_spacing": self.column_spacing,
                "children": [child.to_protocol() for child in self.children],
            }
        )
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Box(Widget):
    orientation: Orientation = Orientation.VERTICAL
    spacing: int = 0
    children: list["TreeNode"] = field(default_factory=list)
    widget_type: str = "box"

    @classmethod
    def vertical(cls, children: list["TreeNode"], spacing: int = 0, **kwargs: object) -> "Box":
        return cls(orientation=Orientation.VERTICAL, spacing=spacing, children=children, **kwargs)

    @classmethod
    def horizontal(cls, children: list["TreeNode"], spacing: int = 0, **kwargs: object) -> "Box":
        return cls(orientation=Orientation.HORIZONTAL, spacing=spacing, children=children, **kwargs)

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "orientation": self.orientation.value,
                "spacing": self.spacing,
                "children": [child.to_protocol() for child in self.children],
            }
        )
        return {"type": self.widget_type, "data": payload}


TreeNode: TypeAlias = (
    Box
    | Grid
    | Scroll
    | Separator
    | Label
    | Image
    | Button
    | Entry
    | Password
    | Switch
    | Scale
    | Dropdown
    | Checkbox
)
