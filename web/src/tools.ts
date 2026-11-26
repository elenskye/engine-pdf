import { bounds, type Color, type Mark, type MarkId, type Point, type Quad, type Shape, type Stroke, type Target, type TextMarkup } from "./plan.js";
import { EditorViewer, type NativeTool, type PageView } from "./viewer.js";

export type CustomTool = "underline" | "strikeout" | "note" | Shape["kind"] | "link";
export type Tool = NativeTool | CustomTool;
export interface ToolStyle {
  color: Color;
  width: number;
  fontSize?: number;
  fill?: Color;
  contents?: string;
  target?: Target;
}

const NATIVE = new Set<Tool>(["select", "highlight", "textBox", "ink", "image", "signature"]);
const SVG = "http://www.w3.org/2000/svg";

export class EditorTools {
  readonly #viewer: EditorViewer;
  readonly #onChange?: () => void;
  readonly #events = new AbortController();
  readonly #adds: Mark[] = [];
  readonly #removals: MarkId[] = [];
  #tool: Tool = "select";
  #style: ToolStyle = { width: 2, color: { red: 0.86, green: 0.22, blue: 0.17, alpha: 1 } };
  #draft: { page: number; view: PageView; start: Point; pointer: number } | null = null;

  constructor(viewer: EditorViewer, onChange?: () => void) {
    this.#viewer = viewer;
    this.#onChange = onChange;
    const container = viewer.container;
    const signal = this.#events.signal;
    for (const name of ["pagerendered", "pagerender", "scalechanging"]) {
      viewer.events.on(name, () => this.#draw(), { signal });
    }
    container.addEventListener("pointerdown", event => this.#start(event), { signal });
    container.addEventListener("pointermove", event => this.#drawDrag(event), { signal });
    container.addEventListener("pointerup", event => this.#end(event), { signal });
    container.addEventListener("pointercancel", () => { this.#draft = null; this.#draw(); }, { signal });
  }

  get tool(): Tool { return this.#tool; }
  get add(): readonly Mark[] { return this.#adds; }
  get remove(): readonly MarkId[] { return this.#removals; }

  async select(tool: Tool): Promise<void> {
    this.#draft = null;
    await this.#viewer.select(NATIVE.has(tool) ? tool as NativeTool : "select");
    this.#tool = tool;
    this.setStyle(this.#style);
  }

  setStyle(style: ToolStyle): void {
    this.#style = style;
    if (NATIVE.has(this.#tool)) {
      const { red, green, blue, alpha } = style.color;
      const color = "#" + [red, green, blue].map(value => Math.round(value * 255).toString(16).padStart(2, "0")).join("");
      this.#viewer.setStyle(this.#tool as NativeTool, { color, opacity: alpha, width: style.width, fontSize: style.fontSize });
    }
  }

  markSelection(kind: TextMarkup): boolean {
    const selection = this.#viewer.container.ownerDocument.getSelection();
    if (!selection?.rangeCount || selection.isCollapsed) return false;
    const range = selection.getRangeAt(0);
    const first = this.#page(range.startContainer);
    const last = this.#page(range.endContainer);
    if (!first || last?.number !== first.number) return false;
    const textLayer = first.view.div.querySelector(".textLayer");
    if (!textLayer?.contains(range.startContainer) || !textLayer.contains(range.endContainer)) return false;
    const quads: Quad[] = [];
    // 只用实际选中文字的矩形，避免换行容器扩大高亮区域。
    const walker = textLayer.ownerDocument.createTreeWalker(textLayer, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
      if (!node.data.trim() || !range.intersectsNode(node)) continue;
      const part = textLayer.ownerDocument.createRange();
      part.setStart(node, node === range.startContainer ? range.startOffset : 0);
      part.setEnd(node, node === range.endContainer ? range.endOffset : node.length);
      for (const rect of part.getClientRects()) {
        if (rect.width < 0.5 || rect.height < 0.5) continue;
        quads.push({
          topLeft: toPdf(first.view, rect.left, rect.top),
          topRight: toPdf(first.view, rect.right, rect.top),
          bottomLeft: toPdf(first.view, rect.left, rect.bottom),
          bottomRight: toPdf(first.view, rect.right, rect.bottom),
        });
      }
    }
    if (!quads.length) return false;
    const rect = bounds(quads.flatMap(quad => Object.values(quad)));
    this.addMark({ kind, source: 0, page: first.number, rect, quads, color: this.#style.color });
    selection.removeAllRanges();
    return true;
  }

  addMark(mark: Mark): void {
    this.#viewer.command(() => { this.#adds.push(mark); this.#changed(); }, () => {
      const index = this.#adds.indexOf(mark);
      if (index >= 0) this.#adds.splice(index, 1);
      this.#changed();
    });
  }

  removeMark(id: MarkId): void {
    if (this.#removals.some(mark => mark.source === id.source && mark.page === id.page && mark.index === id.index)) return;
    this.#viewer.command(() => { this.#removals.push(id); this.#changed(); }, () => {
      const index = this.#removals.indexOf(id);
      if (index >= 0) this.#removals.splice(index, 1);
      this.#changed();
    });
  }

  reset(): void {
    this.#adds.length = this.#removals.length = 0;
    this.#draft = null;
    this.#tool = "select";
    this.#changed();
  }

  destroy(): void {
    this.#events.abort();
    this.reset();
  }

  #changed(): void { this.#draw(); this.#onChange?.(); }

  #page(node: Node | null): { number: number; view: PageView } | undefined {
    const element = node instanceof Element ? node : node?.parentElement;
    const page = element?.closest<HTMLElement>(".page[data-page-number]");
    if (!page || !this.#viewer.container.contains(page)) return undefined;
    const number = Number(page.dataset.pageNumber);
    const view = this.#viewer.pageView(number);
    return view ? { number, view } : undefined;
  }

  #start(event: PointerEvent): void {
    if (event.button !== 0 || NATIVE.has(this.#tool) || this.#tool === "underline" || this.#tool === "strikeout") return;
    const page = this.#page(event.target as Node);
    if (!page) return;
    event.preventDefault();
    const start = toPdf(page.view, event.clientX, event.clientY);
    if (this.#tool === "note") {
      const { color, contents } = this.#style;
      if (!contents?.trim()) return;
      this.addMark({ source: 0, page: page.number, kind: "note", color, contents,
        rect: { x: start.x, y: start.y - 24, width: 24, height: 24 } });
      return;
    }
    this.#draft = { page: page.number, view: page.view, start, pointer: event.pointerId };
    this.#viewer.container.setPointerCapture(event.pointerId);
  }

  #drawDrag(event: PointerEvent): void {
    if (this.#draft?.pointer !== event.pointerId) return;
    this.#draw(this.#dragMark(event));
  }

  #end(event: PointerEvent): void {
    if (this.#draft?.pointer !== event.pointerId) return;
    const mark = this.#dragMark(event);
    this.#draft = null;
    if (mark) this.addMark(mark);
    else this.#draw();
  }

  #dragMark(event: PointerEvent): Mark | undefined {
    const draft = this.#draft;
    if (!draft) return undefined;
    const end = toPdf(draft.view, event.clientX, event.clientY);
    if (Math.hypot(end.x - draft.start.x, end.y - draft.start.y) < 1) return undefined;
    const rect = bounds([draft.start, end]);
    const base = { source: 0, page: draft.page, rect };
    if (this.#tool === "link") {
      return this.#style.target && rect.width > 0 && rect.height > 0
        ? { ...base, kind: "link", target: this.#style.target } : undefined;
    }
    const stroke: Stroke = { width: this.#style.width, color: this.#style.color };
    let shape: Shape;
    switch (this.#tool) {
      case "line":
      case "arrow":
        shape = { kind: this.#tool, start: draft.start, end };
        // PDFKit 需要带面积的标注框，也给箭头留绘制空间。
        const margin = this.#tool === "arrow" ? Math.max(stroke.width, 8) : stroke.width;
        base.rect = { x: rect.x - margin, y: rect.y - margin, width: rect.width + 2 * margin, height: rect.height + 2 * margin };
        break;
      case "rectangle":
      case "ellipse":
        if (rect.width <= 0 || rect.height <= 0) return undefined;
        shape = { kind: this.#tool };
        break;
      default:
        return undefined;
    }
    return { ...base, kind: "shape", shape, stroke, fill: this.#style.fill ?? null };
  }

  #draw(draft?: Mark): void {
    const container = this.#viewer.container;
    container.querySelectorAll("[data-engine-pdf-overlay]").forEach(layer => layer.remove());
    const marks = draft ? [...this.#adds, draft] : this.#adds;
    const layers = new Map<number, SVGSVGElement>();
    for (const mark of marks) {
      if (mark.source !== 0) continue;
      const view = this.#viewer.pageView(mark.page);
      if (!view?.div.querySelector(".canvasWrapper")) continue;
      let layer = layers.get(mark.page);
      if (!layer) {
        layer = container.ownerDocument.createElementNS(SVG, "svg");
        layer.dataset.enginePdfOverlay = "";
        layer.setAttribute("viewBox", [0, 0, view.viewport.width, view.viewport.height].join(" "));
        layer.setAttribute("aria-hidden", "true");
        Object.assign(layer.style, { position: "absolute", inset: "0", width: "100%", height: "100%", pointerEvents: "none", zIndex: "5" });
        view.div.append(layer);
        layers.set(mark.page, layer);
      }
      drawMark(layer, view, mark);
    }
  }
}

function toPdf(view: PageView, clientX: number, clientY: number): Point {
  const element = view.div.querySelector(".canvasWrapper") ?? view.div;
  const box = element.getBoundingClientRect();
  const [x, y] = view.viewport.convertToPdfPoint(
    (clientX - box.left) * view.viewport.width / box.width,
    (clientY - box.top) * view.viewport.height / box.height,
  );
  return { x, y };
}

function drawMark(layer: SVGSVGElement, view: PageView, mark: Mark): void {
  const element = (tag: string, attrs: Record<string, string | number>) => {
    const node = layer.ownerDocument.createElementNS(SVG, tag);
    for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, String(value));
    layer.append(node);
  };
  const project = (point: Point) => view.viewport.convertToViewportPoint(point.x, point.y);
  const css = (color: Color) => "rgba(" + [color.red * 255, color.green * 255, color.blue * 255, color.alpha].join(",") + ")";
  const { x, y, width, height } = mark.rect;
  const corner = project({ x, y });
  const opposite = project({ x: x + width, y: y + height });
  const box = { x: Math.min(corner[0], opposite[0]), y: Math.min(corner[1], opposite[1]),
    width: Math.abs(corner[0] - opposite[0]), height: Math.abs(corner[1] - opposite[1]) };
  if (mark.kind === "highlight" || mark.kind === "underline" || mark.kind === "strikeout") {
    for (const quad of mark.quads) {
      const [tl, tr, bl, br] = [quad.topLeft, quad.topRight, quad.bottomLeft, quad.bottomRight].map(project);
      if (mark.kind === "highlight") {
        element("polygon", { points: [tl, tr, br, bl].map(point => point.join(",")).join(" "), fill: css(mark.color) });
      } else {
        const fraction = mark.kind === "underline" ? 0.93 : 0.53;
        element("line", { x1: tl[0] + fraction * (bl[0] - tl[0]), y1: tl[1] + fraction * (bl[1] - tl[1]),
          x2: tr[0] + fraction * (br[0] - tr[0]), y2: tr[1] + fraction * (br[1] - tr[1]),
          stroke: css(mark.color), "stroke-width": view.viewport.scale });
      }
    }
  } else if (mark.kind === "note" || mark.kind === "link") {
    element("rect", { ...box, fill: mark.kind === "note" ? css(mark.color) : "none",
      stroke: mark.kind === "note" ? "none" : "currentColor", "stroke-dasharray": "4 3" });
  } else if (mark.kind === "shape") {
    const style = { stroke: css(mark.stroke.color), "stroke-width": mark.stroke.width * view.viewport.scale,
      fill: mark.fill ? css(mark.fill) : "none" };
    if (mark.shape.kind === "rectangle") element("rect", { ...box, ...style });
    else if (mark.shape.kind === "ellipse") element("ellipse", { cx: box.x + box.width / 2, cy: box.y + box.height / 2,
      rx: box.width / 2, ry: box.height / 2, ...style });
    else if ("start" in mark.shape) {
      const [x1, y1] = project(mark.shape.start), [x2, y2] = project(mark.shape.end);
      element("line", { x1, y1, x2, y2, ...style });
      if (mark.shape.kind === "arrow") {
        const angle = Math.atan2(y2 - y1, x2 - x1);
        const length = 8 * view.viewport.scale;
        const tips = [-0.55, 0.55].map(offset => [
          x2 - Math.cos(angle + offset) * length, y2 - Math.sin(angle + offset) * length,
        ].join(","));
        element("polyline", { points: [tips[0], [x2, y2].join(","), tips[1]].join(" "), ...style });
      }
    }
  }
}
