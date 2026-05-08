import { Icon, MenuItem } from "./protocol.js";

export type Align = "fill" | "start" | "end" | "center" | "baseline";
export type Orientation = "horizontal" | "vertical";
export type Variant = "normal" | "muted" | "accent" | "success" | "warning" | "danger";

export interface WidgetNode {
  toProtocol(): Record<string, unknown>;
}

export interface CommonProps {
  id?: string;
  visible?: boolean;
  hexpand?: boolean;
  vexpand?: boolean;
  halign?: Align;
  valign?: Align;
  tooltip?: string;
  variant?: Variant;
}

function applyCommonProps(
  payload: Record<string, unknown>,
  props: CommonProps,
): Record<string, unknown> {
  if (props.id !== undefined) payload.id = props.id;
  if (props.visible !== undefined) payload.visible = props.visible;
  if (props.hexpand !== undefined) payload.hexpand = props.hexpand;
  if (props.vexpand !== undefined) payload.vexpand = props.vexpand;
  if (props.halign !== undefined) payload.halign = props.halign;
  if (props.valign !== undefined) payload.valign = props.valign;
  if (props.tooltip !== undefined) payload.tooltip = props.tooltip;
  if (props.variant !== undefined) payload.variant = props.variant;
  return payload;
}

abstract class WidgetBase implements WidgetNode {
  protected constructor(protected readonly common: CommonProps = {}) {}

  protected withCommon(payload: Record<string, unknown>): Record<string, unknown> {
    return applyCommonProps(payload, this.common);
  }

  abstract toProtocol(): Record<string, unknown>;
}

export class Label extends WidgetBase {
  constructor(
    public readonly text: string,
    private readonly options: CommonProps & {
      wrap?: boolean;
      xalign?: number;
      selectable?: boolean;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ text: this.text });
    if (this.options.wrap !== undefined) payload.wrap = this.options.wrap;
    if (this.options.xalign !== undefined) payload.xalign = this.options.xalign;
    if (this.options.selectable !== undefined) payload.selectable = this.options.selectable;
    return { type: "label", data: payload };
  }
}

export class Image extends WidgetBase {
  constructor(
    public readonly icon: Icon,
    private readonly options: CommonProps & { pixel_size?: number } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ icon: this.icon.toProtocol() });
    if (this.options.pixel_size !== undefined) payload.pixel_size = this.options.pixel_size;
    return { type: "image", data: payload };
  }
}

export class IconWidget extends WidgetBase {
  constructor(
    public readonly icon: Icon,
    private readonly options: CommonProps & { pixel_size?: number } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ icon: this.icon.toProtocol() });
    if (this.options.pixel_size !== undefined) payload.pixel_size = this.options.pixel_size;
    return { type: "icon", data: payload };
  }
}

export class Progress extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      value: number;
      max?: number;
      show_text?: boolean;
      text?: string;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      value: this.options.value,
      max: this.options.max ?? 1,
    });
    if (this.options.show_text !== undefined) payload.show_text = this.options.show_text;
    if (this.options.text !== undefined) payload.text = this.options.text;
    return { type: "progress", data: payload };
  }
}

export class Button extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      label?: string;
      icon?: Icon;
      child?: TreeNode;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({});
    if (this.options.label !== undefined) payload.label = this.options.label;
    if (this.options.icon !== undefined) payload.icon = this.options.icon.toProtocol();
    if (this.options.child !== undefined) payload.child = this.options.child.toProtocol();
    return { type: "button", data: payload };
  }
}

export class Switch extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      label?: string;
      active?: boolean;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ active: this.options.active ?? false });
    if (this.options.label !== undefined) payload.label = this.options.label;
    return { type: "switch", data: payload };
  }
}

export class Scale extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      min?: number;
      max?: number;
      step?: number;
      value?: number;
      orientation?: Orientation;
      draw_value?: boolean;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      min: this.options.min ?? 0,
      max: this.options.max ?? 1,
      step: this.options.step ?? 0.1,
      value: this.options.value ?? 0,
    });
    if (this.options.orientation !== undefined) payload.orientation = this.options.orientation;
    if (this.options.draw_value !== undefined) payload.draw_value = this.options.draw_value;
    return { type: "scale", data: payload };
  }
}

export class Checkbox extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      label?: string;
      active?: boolean;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({ active: this.options.active ?? false });
    if (this.options.label !== undefined) payload.label = this.options.label;
    return { type: "checkbox", data: payload };
  }
}

export class DropdownItem {
  constructor(
    public readonly id: string,
    public readonly label: string,
  ) {}

  toProtocol(): Record<string, unknown> {
    return { id: this.id, label: this.label };
  }
}

export class Dropdown extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      items?: DropdownItem[];
      selected?: number;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      items: (this.options.items ?? []).map((item) => item.toProtocol()),
    });
    if (this.options.selected !== undefined) payload.selected = this.options.selected;
    return { type: "dropdown", data: payload };
  }
}

export class Separator extends WidgetBase {
  constructor(private readonly options: CommonProps & { orientation?: Orientation } = {}) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({});
    if (this.options.orientation !== undefined) payload.orientation = this.options.orientation;
    return { type: "separator", data: payload };
  }
}

export class Scroll extends WidgetBase {
  constructor(
    private readonly child: TreeNode,
    options: CommonProps = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return { type: "scroll", data: this.withCommon({ child: this.child.toProtocol() }) };
  }
}

export class GridChild {
  constructor(
    public readonly row: number,
    public readonly column: number,
    public readonly child: TreeNode,
    public readonly width: number = 1,
    public readonly height: number = 1,
  ) {}

  toProtocol(): Record<string, unknown> {
    return {
      row: this.row,
      column: this.column,
      width: this.width,
      height: this.height,
      child: this.child.toProtocol(),
    };
  }
}

export class Grid extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      children?: GridChild[];
      row_spacing?: number;
      column_spacing?: number;
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "grid",
      data: this.withCommon({
        row_spacing: this.options.row_spacing ?? 0,
        column_spacing: this.options.column_spacing ?? 0,
        children: (this.options.children ?? []).map((child) => child.toProtocol()),
      }),
    };
  }
}

export class Hero extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      title: string;
      subtitle: string;
      icon?: Icon;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      title: this.options.title,
      subtitle: this.options.subtitle,
    });
    if (this.options.icon !== undefined) payload.icon = this.options.icon.toProtocol();
    return { type: "hero", data: payload };
  }
}

export class Card extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      children?: TreeNode[];
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "card",
      data: this.withCommon({
        children: (this.options.children ?? []).map((child) => child.toProtocol()),
      }),
    };
  }
}

export class Header {
  constructor(
    public readonly title: string,
    public readonly subtitle = "",
  ) {}

  toProtocol(): Record<string, unknown> {
    const payload: Record<string, unknown> = { title: this.title };
    if (this.subtitle !== "") payload.subtitle = this.subtitle;
    return payload;
  }
}

export class Section extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      title?: string;
      subtitle?: string;
      header?: Header;
      body?: TreeNode[];
      children?: TreeNode[];
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const header =
      this.options.header ??
      (this.options.title === undefined
        ? undefined
        : new Header(this.options.title, this.options.subtitle ?? ""));
    const body = this.options.body ?? this.options.children ?? [];
    return {
      type: "section",
      data: this.withCommon({
        ...(header === undefined ? {} : { header: header.toProtocol() }),
        body: body.map((child) => child.toProtocol()),
      }),
    };
  }
}

export class Collapsible extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      title?: string;
      subtitle?: string;
      header?: Header;
      expanded?: boolean;
      body?: TreeNode[];
      children?: TreeNode[];
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const header =
      this.options.header ??
      (this.options.title === undefined
        ? undefined
        : new Header(this.options.title, this.options.subtitle ?? ""));
    const body = this.options.body ?? this.options.children ?? [];
    return {
      type: "collapsible",
      data: this.withCommon({
        ...(header === undefined ? {} : { header: header.toProtocol() }),
        expanded: this.options.expanded ?? false,
        body: body.map((child) => child.toProtocol()),
      }),
    };
  }
}

export class Item extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      left?: TreeNode;
      label?: string;
      right?: TreeNode;
      clickable?: boolean;
      menu?: MenuItem[];
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      label: this.options.label ?? "",
    });
    if (this.options.left !== undefined) payload.left = this.options.left.toProtocol();
    if (this.options.right !== undefined) payload.right = this.options.right.toProtocol();
    if (this.options.clickable !== undefined) payload.clickable = this.options.clickable;
    if (this.options.menu !== undefined) {
      payload.menu = this.options.menu.map((item) => item.toProtocol());
    }
    return { type: "item", data: payload };
  }
}

export class CollapsibleItem extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      left?: TreeNode;
      label?: string;
      right?: TreeNode;
      expanded?: boolean;
      body?: TreeNode[];
      children?: TreeNode[];
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      label: this.options.label ?? "",
      expanded: this.options.expanded ?? false,
      body: (this.options.body ?? this.options.children ?? []).map((child) => child.toProtocol()),
    });
    if (this.options.left !== undefined) payload.left = this.options.left.toProtocol();
    if (this.options.right !== undefined) payload.right = this.options.right.toProtocol();
    return { type: "collapsible_item", data: payload };
  }
}

export class Meter extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      icon?: Icon;
      label?: string;
      value: number;
      min?: number;
      max?: number;
      step?: number;
      text?: string;
      interactive?: boolean;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      label: this.options.label ?? "",
      value: this.options.value,
      min: this.options.min ?? 0,
      max: this.options.max ?? 1,
      step: this.options.step ?? 0.01,
    });
    if (this.options.icon !== undefined) payload.icon = this.options.icon.toProtocol();
    if (this.options.text !== undefined) payload.text = this.options.text;
    if (this.options.interactive !== undefined) payload.interactive = this.options.interactive;
    return { type: "meter", data: payload };
  }
}

export class Copyable extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      label?: string;
      value: string;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "copyable",
      data: this.withCommon({
        label: this.options.label ?? "",
        value: this.options.value,
      }),
    };
  }
}

export class ToastAction {
  constructor(
    public readonly id: string,
    public readonly label: string,
  ) {}

  toProtocol(): Record<string, unknown> {
    return { id: this.id, label: this.label };
  }
}

export class Toast extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      icon?: Icon;
      title: string;
      message?: string;
      action?: ToastAction;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      title: this.options.title,
      message: this.options.message ?? "",
    });
    if (this.options.icon !== undefined) payload.icon = this.options.icon.toProtocol();
    if (this.options.action !== undefined) payload.action = this.options.action.toProtocol();
    return { type: "toast", data: payload };
  }
}

export class Row extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      title: string;
      subtitle?: string;
      meta?: string;
      icon?: Icon;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    const payload = this.withCommon({
      title: this.options.title,
      subtitle: this.options.subtitle ?? "",
      meta: this.options.meta ?? "",
    });
    if (this.options.icon !== undefined) payload.icon = this.options.icon.toProtocol();
    return { type: "action_row", data: payload };
  }
}

export class DetailGridItem {
  constructor(
    public readonly key: string,
    public readonly value: string,
  ) {}

  toProtocol(): Record<string, unknown> {
    return { key: this.key, value: this.value };
  }
}

export class DetailGrid extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      rows?: DetailGridItem[];
    } = {},
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "detail_grid",
      data: this.withCommon({
        rows: (this.options.rows ?? []).map((row) => row.toProtocol()),
      }),
    };
  }
}

export class EmptyState extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      title: string;
      subtitle?: string;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "empty_state",
      data: this.withCommon({
        title: this.options.title,
        subtitle: this.options.subtitle ?? "",
      }),
    };
  }
}

export class Badge extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      label: string;
    },
  ) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "badge",
      data: this.withCommon({
        label: this.options.label,
      }),
    };
  }
}

export class StatusDot extends WidgetBase {
  constructor(options: CommonProps = {}) {
    super(options);
  }

  toProtocol(): Record<string, unknown> {
    return { type: "status_dot", data: this.withCommon({}) };
  }
}

export class Box extends WidgetBase {
  constructor(
    private readonly options: CommonProps & {
      orientation?: Orientation;
      spacing?: number;
      children?: TreeNode[];
    } = {},
  ) {
    super(options);
  }

  static vertical(children: TreeNode[], spacing = 0, options: CommonProps = {}): Box {
    return new Box({ ...options, orientation: "vertical", spacing, children });
  }

  static horizontal(children: TreeNode[], spacing = 0, options: CommonProps = {}): Box {
    return new Box({ ...options, orientation: "horizontal", spacing, children });
  }

  toProtocol(): Record<string, unknown> {
    return {
      type: "box",
      data: this.withCommon({
        orientation: this.options.orientation ?? "vertical",
        spacing: this.options.spacing ?? 0,
        children: (this.options.children ?? []).map((child) => child.toProtocol()),
      }),
    };
  }
}

export type TreeNode =
  | Hero
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
  | Checkbox;
