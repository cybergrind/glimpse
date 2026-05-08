export type IconKind = "name" | "path";

export class Icon {
  constructor(
    public readonly kind: IconKind,
    public readonly value: string,
  ) {}

  static name(value: string): Icon {
    return new Icon("name", value);
  }

  static path(value: string): Icon {
    return new Icon("path", value);
  }

  toProtocol(): { name: string } | { path: string } {
    return this.kind === "name" ? { name: this.value } : { path: this.value };
  }
}

export class StatusItem {
  constructor(
    public readonly options: {
      id?: string;
      icon?: Icon;
      label?: string;
      tooltip?: string;
    } = {},
  ) {}

  toProtocol(): Record<string, unknown> {
    const payload: Record<string, unknown> = {};
    if (this.options.id !== undefined) {
      payload.id = this.options.id;
    }
    if (this.options.icon !== undefined) {
      payload.icon = this.options.icon.toProtocol();
    }
    if (this.options.label !== undefined) {
      payload.label = this.options.label;
    }
    if (this.options.tooltip !== undefined) {
      payload.tooltip = this.options.tooltip;
    }
    return payload;
  }
}

export class MenuItem {
  constructor(
    public readonly options: {
      id: string;
      label: string;
      visible?: boolean;
      enabled?: boolean;
    },
  ) {}

  toProtocol(): Record<string, unknown> {
    const payload: Record<string, unknown> = {
      id: this.options.id,
      label: this.options.label,
    };
    if (this.options.visible !== undefined) {
      payload.visible = this.options.visible;
    }
    if (this.options.enabled !== undefined) {
      payload.enabled = this.options.enabled;
    }
    return payload;
  }
}
