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


class Variant(StrEnum):
    NORMAL = "normal"
    MUTED = "muted"
    ACCENT = "accent"
    SUCCESS = "success"
    WARNING = "warning"
    DANGER = "danger"


@dataclass(slots=True)
class CommonProps:
    id: str | None = None
    visible: bool | None = None
    hexpand: bool | None = None
    vexpand: bool | None = None
    halign: Align | None = None
    valign: Align | None = None
    tooltip: str | None = None
    variant: Variant | None = None

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
        if self.variant is not None:
            payload["variant"] = self.variant.value
        return payload


class Widget(CommonProps):
    widget_type: str = ""

    def to_protocol(self) -> dict[str, object]:
        raise NotImplementedError


@dataclass(slots=True)
class Hero(Widget):
    title: str = ""
    subtitle: str = ""
    icon: Icon | None = None
    widget_type: str = "hero"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({
            "title": self.title,
            "subtitle": self.subtitle,
        })
        if self.icon is not None:
            payload["icon"] = self.icon.to_protocol()
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class IconWidget(Widget):
    icon: Icon | None = None
    pixel_size: int | None = None
    widget_type: str = "icon"

    def to_protocol(self) -> dict[str, object]:
        if self.icon is None:
            raise ValueError("IconWidget requires an icon")
        payload = self.apply_common({"icon": self.icon.to_protocol()})
        if self.pixel_size is not None:
            payload["pixel_size"] = self.pixel_size
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Progress(Widget):
    value: float = 0.0
    max: float = 1.0
    show_text: bool = False
    text: str | None = None
    widget_type: str = "progress"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"value": self.value, "max": self.max})
        if self.show_text:
            payload["show_text"] = self.show_text
        if self.text is not None:
            payload["text"] = self.text
        return {"type": self.widget_type, "data": payload}


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


@dataclass(slots=True)
class Card(Widget):
    children: list["TreeNode"] = field(default_factory=list)
    widget_type: str = "card"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"children": [child.to_protocol() for child in self.children]})
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Header:
    title: str
    subtitle: str = ""

    def to_protocol(self) -> dict[str, object]:
        payload: dict[str, object] = {"title": self.title}
        if self.subtitle:
            payload["subtitle"] = self.subtitle
        return payload


@dataclass(slots=True)
class Section(Widget):
    title: str = ""
    subtitle: str = ""
    header: Header | None = None
    body: list["TreeNode"] = field(default_factory=list)
    children: list["TreeNode"] = field(default_factory=list)
    widget_type: str = "section"

    def to_protocol(self) -> dict[str, object]:
        header = self.header
        if header is None and (self.title or self.subtitle):
            header = Header(self.title, self.subtitle)
        body = self.body or self.children
        payload = self.apply_common(
            {
                "body": [child.to_protocol() for child in body],
            }
        )
        if header is not None:
            payload["header"] = header.to_protocol()
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Collapsible(Widget):
    title: str = ""
    subtitle: str = ""
    header: Header | None = None
    expanded: bool = False
    body: list["TreeNode"] = field(default_factory=list)
    children: list["TreeNode"] = field(default_factory=list)
    widget_type: str = "collapsible"

    def to_protocol(self) -> dict[str, object]:
        header = self.header
        if header is None and (self.title or self.subtitle):
            header = Header(self.title, self.subtitle)
        body = self.body or self.children
        payload = self.apply_common(
            {
                "expanded": self.expanded,
                "body": [child.to_protocol() for child in body],
            }
        )
        if header is not None:
            payload["header"] = header.to_protocol()
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Item(Widget):
    left: "TreeNode | None" = None
    label: str = ""
    right: "TreeNode | None" = None
    clickable: bool = False
    widget_type: str = "item"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"label": self.label})
        if self.left is not None:
            payload["left"] = self.left.to_protocol()
        if self.right is not None:
            payload["right"] = self.right.to_protocol()
        if self.clickable:
            payload["clickable"] = self.clickable
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class CollapsibleItem(Widget):
    left: "TreeNode | None" = None
    label: str = ""
    right: "TreeNode | None" = None
    expanded: bool = False
    body: list["TreeNode"] = field(default_factory=list)
    children: list["TreeNode"] = field(default_factory=list)
    widget_type: str = "collapsible_item"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "label": self.label,
                "expanded": self.expanded,
                "body": [child.to_protocol() for child in (self.body or self.children)],
            }
        )
        if self.left is not None:
            payload["left"] = self.left.to_protocol()
        if self.right is not None:
            payload["right"] = self.right.to_protocol()
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Meter(Widget):
    icon: Icon | None = None
    label: str = ""
    value: float = 0.0
    min: float = 0.0
    max: float = 1.0
    step: float = 0.01
    text: str | None = None
    interactive: bool = False
    widget_type: str = "meter"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {
                "label": self.label,
                "value": self.value,
                "min": self.min,
                "max": self.max,
                "step": self.step,
            }
        )
        if self.icon is not None:
            payload["icon"] = self.icon.to_protocol()
        if self.text is not None:
            payload["text"] = self.text
        if self.interactive:
            payload["interactive"] = self.interactive
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Copyable(Widget):
    label: str = ""
    value: str = ""
    widget_type: str = "copyable"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"label": self.label, "value": self.value})
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class ToastAction:
    id: str
    label: str

    def to_protocol(self) -> dict[str, str]:
        return {"id": self.id, "label": self.label}


@dataclass(slots=True)
class Toast(Widget):
    icon: Icon | None = None
    title: str = ""
    message: str = ""
    action: ToastAction | None = None
    widget_type: str = "toast"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"title": self.title, "message": self.message})
        if self.icon is not None:
            payload["icon"] = self.icon.to_protocol()
        if self.action is not None:
            payload["action"] = self.action.to_protocol()
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Row(Widget):
    title: str = ""
    subtitle: str = ""
    meta: str = ""
    icon: Icon | None = None
    widget_type: str = "action_row"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common(
            {"title": self.title, "subtitle": self.subtitle, "meta": self.meta}
        )
        if self.icon is not None:
            payload["icon"] = self.icon.to_protocol()
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class DetailGridItem:
    key: str
    value: str

    def to_protocol(self) -> dict[str, str]:
        return {"key": self.key, "value": self.value}


@dataclass(slots=True)
class DetailGrid(Widget):
    rows: list[DetailGridItem] = field(default_factory=list)
    widget_type: str = "detail_grid"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"rows": [row.to_protocol() for row in self.rows]})
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class EmptyState(Widget):
    title: str = ""
    subtitle: str = ""
    widget_type: str = "empty_state"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"title": self.title, "subtitle": self.subtitle})
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class Badge(Widget):
    label: str = ""
    widget_type: str = "badge"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({"label": self.label})
        return {"type": self.widget_type, "data": payload}


@dataclass(slots=True)
class StatusDot(Widget):
    widget_type: str = "status_dot"

    def to_protocol(self) -> dict[str, object]:
        payload = self.apply_common({})
        return {"type": self.widget_type, "data": payload}


TreeNode: TypeAlias = (
    Hero
    | Card
    | Section
    | Collapsible
    | Item
    | CollapsibleItem
    | Meter
    | Copyable
    | Toast
    | Row
    | DetailGrid
    | EmptyState
    | Badge
    | StatusDot
    | Box
    | Grid
    | Scroll
    | Progress
    | Separator
    | Label
    | IconWidget
    | Image
    | Button
    | Switch
    | Scale
    | Dropdown
    | Checkbox
)
