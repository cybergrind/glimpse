export type IconKind = "name" | "path";

export class Icon {
  constructor(
    public readonly type: IconKind,
    public readonly value: string,
  ) {}

  static name(value: string): Icon {
    return new Icon("name", value);
  }

  static path(value: string): Icon {
    return new Icon("path", value);
  }

  toProtocol(): { type: IconKind; value: string } {
    return { type: this.type, value: this.value };
  }
}

export class StatusItem {
  constructor(
    public readonly options: {
      id?: string;
      icon?: Icon;
      text?: string;
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
    if (this.options.text !== undefined) {
      payload.text = this.options.text;
    }
    return payload;
  }
}

