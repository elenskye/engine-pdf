import type { Field, Fill } from "./plan.js";

interface Widget {
  id: string;
  exportValues?: string | string[];
}

export function captureFields(
  fields: readonly Field[],
  widgets: ReadonlyMap<string, readonly object[]> | null,
  storage: ReadonlyMap<string, unknown>,
): Fill[] {
  const fills: Fill[] = [];
  for (const field of fields) {
    if (field.readOnly || field.kind === "other") continue;
    const entries = (widgets?.get(field.name) ?? []) as readonly Widget[];
    const changed = entries.flatMap(widget => {
      const entry = storage.get(widget.id) as { value?: unknown } | undefined;
      return entry?.value === undefined ? [] : [{ widget, value: entry.value }];
    });
    let value: Fill["value"] | undefined;
    switch (field.kind) {
      case "checkbox":
        if (changed.length) value = changed.some(entry => entry.value === true ||
          (typeof entry.value === "string" && entry.value !== "Off" && entry.value === entry.widget.exportValues));
        break;
      case "radio":
        for (const entry of changed) {
          const selected = entry.value === true ? entry.widget.exportValues : entry.value;
          if (typeof selected === "string" && selected !== "Off") value = selected;
        }
        break;
      case "text":
      case "dropdown":
        for (const entry of changed) {
          const selected = Array.isArray(entry.value) ? entry.value[0] : entry.value;
          if (typeof selected === "string") value = selected;
        }
        break;
    }
    if (value !== undefined && value !== field.value) fills.push({ name: field.name, value });
  }
  return fills;
}
