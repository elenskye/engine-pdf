import type { PDFDocumentProxy } from "pdfjs-dist";
import type { Pick } from "./plan.js";

export class PageOrganizer {
  #pages: Pick[];
  #nextSource = 1;
  readonly originalCount: number;

  constructor(pageCount: number) {
    if (!Number.isInteger(pageCount) || pageCount < 1) throw new RangeError("需要至少一页。");
    this.originalCount = pageCount;
    this.#pages = Array.from({ length: pageCount }, (_, index) => ({ source: 0, page: index + 1, rotate: 0 }));
  }

  get pages(): readonly Pick[] {
    return this.#pages.map(page => ({ ...page }));
  }

  plan(): Pick[] | undefined {
    return this.#pages.length === this.originalCount &&
      this.#pages.every(({ source, page, rotate }, index) => source === 0 && page === index + 1 && rotate === 0)
      ? undefined : this.#pages.map(page => ({ ...page }));
  }

  insert(pageCount: number, at = this.#pages.length): number {
    if (!Number.isInteger(pageCount) || pageCount < 1 || !Number.isInteger(at) || at < 0 || at > this.#pages.length) {
      throw new RangeError("插入位置或页数无效。");
    }
    const source = this.#nextSource++;
    this.#pages = [
      ...this.#pages.slice(0, at),
      ...Array.from({ length: pageCount }, (_, index) => ({ source, page: index + 1, rotate: 0 })),
      ...this.#pages.slice(at),
    ];
    return source;
  }

  remove(index: number): void {
    this.#at(index);
    if (this.#pages.length === 1) throw new RangeError("至少保留一页。");
    this.#pages.splice(index, 1);
  }

  duplicate(index: number): void {
    const page = this.#at(index);
    this.#pages.splice(index + 1, 0, { ...page });
  }

  move(from: number, to: number): void {
    const page = this.#at(from);
    this.#at(to);
    this.#pages.splice(from, 1);
    this.#pages.splice(to, 0, page);
  }

  rotate(index: number, degrees = 90): void {
    if (!Number.isInteger(degrees) || degrees % 90) throw new RangeError("旋转角度须为 90 的倍数。");
    const page = this.#at(index);
    page.rotate = ((page.rotate + degrees) % 360 + 360) % 360;
  }

  #at(index: number): Pick {
    if (!Number.isInteger(index) || index < 0 || index >= this.#pages.length) throw new RangeError("找不到该页。");
    return this.#pages[index];
  }
}

export async function renderThumbnail(document: PDFDocumentProxy, pick: Pick, canvas: HTMLCanvasElement, side = 128): Promise<void> {
  const page = await document.getPage(pick.page);
  const rotation = (page.rotate + pick.rotate) % 360;
  const original = page.getViewport({ scale: 1, rotation });
  const viewport = page.getViewport({ scale: side / Math.max(original.width, original.height), rotation });
  canvas.width = Math.ceil(viewport.width);
  canvas.height = Math.ceil(viewport.height);
  await page.render({ canvas, viewport }).promise;
}
