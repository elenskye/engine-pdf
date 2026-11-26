import { bounds, type Color, type Mark, type MarkId, type MarkKind, type Point, type Quad, type ReadAnnotation, type Rect } from "./plan.js";

// PDF.js 6.5 的编辑器序列化格式。数值来自该版本的 AnnotationEditorType。
interface NativeEdit {
  annotationType?: number;
  id?: string;
  deleted?: boolean;
  pageIndex?: number;
  rect?: ArrayLike<number>;
  color?: ArrayLike<number>;
  opacity?: number;
  fontSize?: number;
  value?: string;
  thickness?: number;
  quadPoints?: ArrayLike<number>;
  paths?: { points?: ArrayLike<number>[]; lines?: ArrayLike<number>[] };
  bitmapId?: string;
  bitmap?: ImageBitmap;
  isSignature?: boolean;
  areContours?: boolean;
  lines?: ArrayLike<number>[];
  popup?: { contents?: string; deleted?: boolean };
}

export interface CaptureOptions {
  source?: number;
  resolveExisting?: (id: string, page: number) => ReadAnnotation | undefined | Promise<ReadAnnotation | undefined>;
  imageBytes?: (bitmap: ImageBitmap) => Promise<number[]>;
  signatureBytes?: (lines: readonly ArrayLike<number>[], rect: Rect, thickness: number, contours: boolean) => Promise<number[]>;
}

export async function captureEditors(
  storage: ReadonlyMap<string, unknown>,
  options: CaptureOptions = {},
): Promise<{ add: Mark[]; remove: MarkId[] }> {
  const edits = [...storage.values()].filter((entry): entry is NativeEdit =>
    typeof entry === "object" && entry !== null && ("annotationType" in entry || "deleted" in entry));
  const bitmaps = new Map(edits.flatMap(edit =>
    edit.bitmapId && edit.bitmap ? [[edit.bitmapId, edit.bitmap] as const] : []));
  const add: Mark[] = [];
  const remove: MarkId[] = [];
  for (const edit of edits) {
    if (!Number.isInteger(edit.pageIndex) || edit.pageIndex! < 0) continue;
    const page = edit.pageIndex! + 1;
    const original = edit.id ? await options.resolveExisting?.(edit.id, page) : undefined;
    if (edit.id && !original) throw new Error("无法定位被编辑的原标注。");
    if (original) remove.push(original.id);
    if (edit.deleted) continue;
    const rect = edit.rect ? fromNativeRect(edit.rect) : original?.rect;
    if (!rect) throw new Error("标注缺少位置。");
    const mark = edit.rect
      ? await nativeMark(edit, rect, bitmaps, options, original?.mark)
      : original?.mark;
    if (!mark) throw new Error("当前标注不能转换为改动单。");
    const contents = edit.popup ? edit.popup.deleted ? undefined : edit.popup.contents : original?.contents ?? undefined;
    add.push({
      ...mark, rect, page, source: options.source ?? 0,
      ...(original?.author != null && { author: original.author }),
      ...(contents !== undefined && { contents }),
    });
  }
  return { add, remove };
}

function fromNativeRect(rect: ArrayLike<number>): Rect {
  if (rect.length !== 4 || !Array.from(rect).every(Number.isFinite)) throw new Error("标注位置无效。");
  return bounds([{ x: rect[0], y: rect[1] }, { x: rect[2], y: rect[3] }]);
}

function fromNativeColor(color?: ArrayLike<number>, alpha = 1): Color {
  const [red, green, blue] = color ? Array.from(color, value => value / 255) : [0, 0, 0];
  return { red, green, blue, alpha };
}

async function nativeMark(
  edit: NativeEdit,
  rect: Rect,
  bitmaps: ReadonlyMap<string, ImageBitmap>,
  options: CaptureOptions,
  original?: MarkKind,
): Promise<MarkKind | undefined> {
  let color = fromNativeColor(edit.color, edit.opacity);
  if (edit.opacity === undefined && original?.kind === "textBox") color = { ...color, alpha: original.color.alpha };
  switch (edit.annotationType) {
    case 3:
      return { kind: "textBox", text: edit.value ?? "", fontSize: edit.fontSize ?? 12, color,
        border: original?.kind === "textBox" ? original.border : null };
    case 9: {
      const coords = edit.quadPoints;
      if (!coords?.length || coords.length % 8 !== 0) throw new Error("请选择文字后再创建高亮。");
      const quads: Quad[] = [];
      for (let index = 0; index < coords.length; index += 8) {
        const point = (offset: number) => ({ x: coords[index + offset], y: coords[index + offset + 1] });
        quads.push({ topLeft: point(0), topRight: point(2), bottomLeft: point(4), bottomRight: point(6) });
      }
      return { kind: "highlight", quads, color };
    }
    case 15: {
      const strokes = (edit.paths?.points ?? []).map(coordinates => toPoints(coordinates)).filter(stroke => stroke.length > 1);
      if (!strokes.length) return original;
      return { kind: "ink", strokes, stroke: { width: edit.thickness ?? 1, color } };
    }
    case 13: {
      const bitmap = edit.bitmap ?? (edit.bitmapId ? bitmaps.get(edit.bitmapId) : undefined);
      if (!bitmap) return original;
      const bytes = await (options.imageBytes ?? encodeBitmap)(bitmap);
      return { kind: "image", format: "png", bytes };
    }
    case 101: {
      if (!edit.lines?.length) return original;
      const bytes = await (options.signatureBytes ?? encodeSignature)(edit.lines, rect, edit.thickness ?? 1, !!edit.areContours);
      return { kind: "image", format: "png", bytes };
    }
    default:
      return original;
  }
}

function toPoints(coordinates: ArrayLike<number>): Point[] {
  const points: Point[] = [];
  for (let i = 0; i + 1 < coordinates.length; i += 2) {
    if (Number.isFinite(coordinates[i]) && Number.isFinite(coordinates[i + 1])) {
      points.push({ x: coordinates[i], y: coordinates[i + 1] });
    }
  }
  return points;
}

async function encodeBitmap(bitmap: ImageBitmap): Promise<number[]> {
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  canvas.getContext("2d")!.drawImage(bitmap, 0, 0);
  return Array.from(new Uint8Array(await (await canvas.convertToBlob()).arrayBuffer()));
}

async function encodeSignature(
  lines: readonly ArrayLike<number>[], rect: Rect, thickness: number, contours: boolean,
): Promise<number[]> {
  if (rect.width <= 0 || rect.height <= 0) throw new Error("签名范围无效。");
  const scale = Math.min(3, 2048 / Math.max(rect.width, rect.height));
  const canvas = new OffscreenCanvas(Math.ceil(rect.width * scale), Math.ceil(rect.height * scale));
  const context = canvas.getContext("2d")!;
  context.setTransform(scale, 0, 0, -scale, -rect.x * scale, (rect.y + rect.height) * scale);
  context.lineWidth = thickness;
  context.lineJoin = context.lineCap = "round";
  for (const line of lines) {
    if (line.length < 6) continue;
    context.moveTo(line[4], line[5]);
    for (let i = 6; i + 5 < line.length; i += 6) {
      if (Number.isNaN(line[i])) context.lineTo(line[i + 4], line[i + 5]);
      else context.bezierCurveTo(line[i], line[i + 1], line[i + 2], line[i + 3], line[i + 4], line[i + 5]);
    }
    if (contours) context.closePath();
    else if (line.length === 6) context.lineTo(line[4] + 0.01, line[5]);
  }
  if (contours) context.fill("evenodd");
  else context.stroke();
  return Array.from(new Uint8Array(await (await canvas.convertToBlob()).arrayBuffer()));
}
