import type { OutlineItem } from "./plan.js";

export function remapOutline(items: readonly OutlineItem[], mapPage: (page: number) => number | undefined): OutlineItem[] {
  return items.map(({ title, target, children }) => {
    if (target.kind === "page") {
      const page = mapPage(target.page);
      target = page === undefined ? { kind: "none" } : { ...target, page };
    }
    return { title, target: { ...target }, ...(children?.length && { children: remapOutline(children, mapPage) }) };
  });
}

export class OutlineEditor {
  #items: OutlineItem[];
  #changed = false;

  constructor(items: readonly OutlineItem[]) {
    this.#items = remapOutline(items, page => page);
  }

  get items(): OutlineItem[] {
    return remapOutline(this.#items, page => page);
  }

  plan(): OutlineItem[] | undefined {
    return this.#changed ? this.items : undefined;
  }

  insert(parent: readonly number[], item: OutlineItem): void {
    this.#children(parent).push(remapOutline([item], page => page)[0]);
    this.#changed = true;
  }

  rename(path: readonly number[], title: string): void {
    this.#item(path).title = title;
    this.#changed = true;
  }

  remove(path: readonly number[]): void {
    this.#item(path);
    this.#children(path.slice(0, -1)).splice(path[path.length - 1], 1);
    this.#changed = true;
  }

  move(path: readonly number[], parent: readonly number[], index: number): void {
    const item = this.#item(path);
    if (parent.length >= path.length && path.every((part, i) => part === parent[i])) {
      throw new RangeError("目录不能移入自己的下级。");
    }
    const origin = this.#children(path.slice(0, -1));
    const destination = this.#children(parent);
    const length = destination.length - Number(origin === destination);
    if (!Number.isInteger(index) || index < 0 || index > length) throw new RangeError("目录位置无效。");
    origin.splice(path[path.length - 1], 1);
    destination.splice(index, 0, item);
    this.#changed = true;
  }

  // 未编辑的目录由 Rust 随页序映射。仅调整已经编辑的输出页码。
  remap(mapPage: (page: number) => number | undefined): void {
    this.#items = remapOutline(this.#items, mapPage);
  }

  #item(path: readonly number[]): OutlineItem {
    if (!path.length) throw new RangeError("缺少目录项。");
    let items: OutlineItem[] | undefined = this.#items;
    let item: OutlineItem | undefined;
    for (const index of path) {
      item = items?.[index];
      if (!item) throw new RangeError("找不到目录项。");
      items = item.children;
    }
    return item!;
  }

  #children(path: readonly number[]): OutlineItem[] {
    return path.length ? this.#item(path).children ??= [] : this.#items;
  }
}
