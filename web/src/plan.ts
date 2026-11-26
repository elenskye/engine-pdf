// 与 Rust edit::Plan 的 JSON 一致。坐标是 PDF 点，物理页码从 1 起。
export interface Point { x: number; y: number }
export interface Rect { x: number; y: number; width: number; height: number }
export interface Quad { topLeft: Point; topRight: Point; bottomLeft: Point; bottomRight: Point }
export interface Color { red: number; green: number; blue: number; alpha: number }
export interface Stroke { width: number; color: Color }
export interface Pick { source: number; page: number; rotate: number }
export interface MarkId { source: number; page: number; index: number }
export interface MarkLocation { source: number; page: number; rect: Rect; author?: string; contents?: string }

export type Shape = { kind: "rectangle" | "ellipse" }
  | { kind: "line" | "arrow"; start: Point; end: Point };

export type Target = { kind: "page"; page: number; y: number }
  | { kind: "url"; url: string }
  | { kind: "none" };

export type TextMarkup = "highlight" | "underline" | "strikeout";
export type MarkKind =
  | { kind: TextMarkup; quads: Quad[]; color: Color }
  | { kind: "note"; color: Color }
  | { kind: "textBox"; text: string; fontSize: number; color: Color; border: Stroke | null }
  | { kind: "ink"; strokes: Point[][]; stroke: Stroke }
  | { kind: "shape"; shape: Shape; stroke: Stroke; fill: Color | null }
  | { kind: "image"; format: "png" | "jpeg"; bytes: number[] }
  | { kind: "link"; target: Target };
export type Mark = MarkLocation & MarkKind;

export type ReadAnnotation = {
  id: MarkId; rect: Rect; author: string | null; contents: string | null;
  color: Color | null; mark?: MarkKind;
} & (
  | { kind: TextMarkup | "note" | "textBox" | "ink" | "rectangle" | "ellipse" | "line" | "arrow" | "image" }
  | { kind: "link"; target: Target }
  | { kind: "other"; subtype: string }
);

export type FieldValue = string | boolean;
export interface Fill { name: string; value: FieldValue }
export interface Field {
  name: string;
  kind: "text" | "checkbox" | "radio" | "dropdown" | "other";
  value: FieldValue | null;
  options: { value: string; label: string }[];
  widgets: { page: number; rect: Rect }[];
  readOnly: boolean;
}
export interface OutlineItem { title: string; target: Target; children?: OutlineItem[] }
export interface InfoPatch {
  title?: string;
  author?: string;
  subject?: string;
  keywords?: string[];
  creator?: string;
}
export interface Plan {
  pages?: Pick[];
  add?: Mark[];
  remove?: MarkId[];
  fill?: Fill[];
  outline?: OutlineItem[];
  info?: InfoPatch;
}

export function bounds(points: readonly Point[]): Rect {
  if (!points.length) throw new RangeError("标注没有坐标。");
  let left = points[0].x, right = left, bottom = points[0].y, top = bottom;
  for (const { x, y } of points) {
    left = Math.min(left, x);
    right = Math.max(right, x);
    bottom = Math.min(bottom, y);
    top = Math.max(top, y);
  }
  return { x: left, y: bottom, width: right - left, height: top - bottom };
}

export function compactPlan(plan: Plan): Plan {
  return {
    ...(plan.pages !== undefined && { pages: plan.pages }),
    ...(plan.add?.length && { add: plan.add }),
    ...(plan.remove?.length && { remove: plan.remove }),
    ...(plan.fill?.length && { fill: plan.fill }),
    ...(plan.outline !== undefined && { outline: plan.outline }),
    ...(plan.info !== undefined && { info: plan.info }),
  };
}
