import {
  AnnotationEditorParamsType as Params, AnnotationEditorType as Mode, AnnotationMode,
  GlobalWorkerOptions, getDocument, type AnnotationEditorUIManager,
  type PDFDocumentLoadingTask, type PDFDocumentProxy, type PDFPageProxy,
} from "pdfjs-dist";
import { EventBus, PDFFindController, PDFLinkService, PDFViewer } from "pdfjs-dist/web/pdf_viewer.mjs";
import { captureEditors } from "./capture.js";
import { captureFields } from "./forms.js";
import type { Field, Plan, Point, ReadAnnotation } from "./plan.js";

export interface ViewerAssets {
  worker: string;
  images?: string;
  cmaps?: string;
  fonts?: string;
  wasm?: string;
  iccs?: string;
}
export interface ViewerOptions {
  container: HTMLDivElement;
  viewer: HTMLDivElement;
  assets: ViewerAssets;
  annotations?: (page: number) => Promise<readonly ReadAnnotation[]> | readonly ReadAnnotation[];
  onPage?: (number: number, label: string | null) => void;
  onScale?: (scale: number) => void;
  onFind?: (current: number, total: number) => void;
  onChange?: () => void;
}

export type NativeTool = "select" | "highlight" | "textBox" | "ink" | "image" | "signature";
type NativeStyle = { color?: string; opacity?: number; width?: number; fontSize?: number };
export type SignatureInput =
  | { kind: "text"; text: string; fontFamily?: string }
  | { kind: "draw"; strokes: Point[][]; width: number; height: number; thickness: number }
  | { kind: "image"; file: Blob };
export interface PageView {
  div: HTMLDivElement;
  viewport: ReturnType<PDFPageProxy["getViewport"]>;
}

interface SignatureEditor {
  getFromText(text: string, font: { fontFamily: string; fontStyle: string; fontWeight: string }): unknown;
  getDrawnSignature(input: { curves: { points: number[] }[]; width: number; height: number; thickness: number }): unknown;
  getFromImage(image: ImageBitmap): unknown;
  addSignature(data: unknown, height: number, description: string): void;
  remove(): void;
}

const MODES: Record<NativeTool, number> = {
  select: Mode.NONE, highlight: Mode.HIGHLIGHT, textBox: Mode.FREETEXT,
  ink: Mode.INK, image: Mode.STAMP, signature: Mode.SIGNATURE,
};

export class EditorViewer {
  readonly events = new EventBus();
  readonly container: HTMLDivElement;
  readonly #viewer: PDFViewer;
  readonly #link: PDFLinkService;
  readonly #options: ViewerOptions;
  readonly #listeners = new AbortController();
  #load: PDFDocumentLoadingTask | null = null;
  #pdf: PDFDocumentProxy | null = null;
  #native: AnnotationEditorUIManager | null = null;
  #editorReady = false;
  #pendingStyle: { tool: NativeTool; style: NativeStyle } | null = null;
  #labels: string[] | null = null;
  #signature: { input: SignatureInput; resolve: () => void; reject: (error: unknown) => void } | null = null;
  #originals = new Map<number, Promise<ReadonlyMap<string, ReadAnnotation>>>();

  constructor(options: ViewerOptions) {
    this.container = options.container;
    this.#options = options;
    GlobalWorkerOptions.workerSrc = options.assets.worker;
    this.container.setAttribute("lang", "");
    this.#link = new PDFLinkService({ eventBus: this.events });
    this.#link.externalLinkEnabled = false;
    const finder = new PDFFindController({ eventBus: this.events, linkService: this.#link });
    const signatureManager = {
      loadSignatures() {},
      destroy() {},
      renderEditButton() { return null; },
      getSignature: ({ editor }: { editor: SignatureEditor }) => { void this.#placeSignature(editor); },
    };
    const viewerOptions = {
      container: this.container, viewer: options.viewer, eventBus: this.events,
      linkService: this.#link, findController: finder, annotationMode: AnnotationMode.ENABLE_FORMS,
      annotationEditorMode: Mode.NONE, imageResourcesPath: options.assets.images,
      annotationEditorHighlightColors: "yellow=#FFFF98,green=#53FFBC,blue=#80EBFF,pink=#FFCBE6,red=#FF4F5F",
      enableHighlightFloatingButton: false, signatureManager,
    };
    this.#viewer = new PDFViewer(viewerOptions);
    this.#link.setViewer(this.#viewer);
    const signal = this.#listeners.signal;
    this.events.on("annotationeditoruimanager", ({ uiManager }: { uiManager: AnnotationEditorUIManager }) => {
      this.#native = uiManager;
    }, { signal });
    this.events.on("annotationeditorlayerrendered", ({ error }: { error?: unknown }) => {
      if (this.#editorReady || error) return;
      this.#editorReady = true;
      const pending = this.#pendingStyle;
      this.#pendingStyle = null;
      if (pending) this.setStyle(pending.tool, pending.style);
    }, { signal });
    this.events.on("switchannotationeditormode", ({ mode, editId, isFromKeyboard, mustEnterInEditMode, editComment }: {
      mode: number; editId?: string; isFromKeyboard?: boolean; mustEnterInEditMode?: boolean; editComment?: boolean;
    }) => {
      if (this.#native) this.#viewer.annotationEditorMode = { mode, editId, isFromKeyboard, mustEnterInEditMode, editComment };
    }, { signal });
    this.events.on("pagechanging", ({ pageNumber }: { pageNumber: number }) => {
      options.onPage?.(pageNumber, this.label(pageNumber));
    }, { signal });
    this.events.on("scalechanging", ({ scale }: { scale: number }) => options.onScale?.(scale), { signal });
    for (const name of ["updatefindmatchescount", "updatefindcontrolstate"]) {
      this.events.on(name, ({ matchesCount }: { matchesCount?: { current: number; total: number } }) =>
        options.onFind?.(matchesCount?.current ?? 0, matchesCount?.total ?? 0), { signal });
    }
    this.events.on("annotationeditorstateschanged", () => options.onChange?.(), { signal });
    this.container.addEventListener("input", () => options.onChange?.(), { signal });
    this.container.addEventListener("change", () => options.onChange?.(), { signal });
    this.container.addEventListener("pointerdown", event => {
      const element = event.target as Element | null;
      if (this.#native?.getMode() === Mode.HIGHLIGHT &&
          element?.matches?.(".textLayer, .textLayer :is([role='img'], .endOfContent, .textLayerImages, .textLayerImagePlaceholder)")) {
        event.stopPropagation();
      }
    }, { capture: true, signal });
    for (const name of ["click", "dblclick"]) {
      this.container.addEventListener(name, event => {
        const element = event.target as Element | null;
        const link = element?.closest?.(".linkAnnotation a");
        const href = link?.getAttribute("href") ?? "";
        // PDF.js 用空锚点绑定附件、重置表单和命名动作。只放行文档内导航。
        if (element?.closest?.(".fileAttachmentAnnotation, .pushButton") ||
            (link && (!href.startsWith("#") || href.length === 1))) {
          event.preventDefault();
          event.stopPropagation();
        }
      }, { capture: true, signal });
    }
  }

  get document(): PDFDocumentProxy | null { return this.#pdf; }
  get pageNumber(): number { return this.#viewer.currentPageNumber; }

  label(page: number): string | null { return this.#labels?.[page - 1] ?? null; }
  pageView(page: number): PageView | undefined { return this.#viewer.getPageView(page - 1) as PageView | undefined; }
  goToPage(page: number): void {
    if (!Number.isInteger(page) || page < 1 || page > (this.#pdf?.numPages ?? 0)) throw new RangeError("页码越界。");
    this.#viewer.currentPageNumber = page;
  }
  zoom(scale: number | "page-fit" | "page-width"): void { this.#viewer.currentScaleValue = String(scale); }
  zoomBy(steps: number): void { this.#viewer.updateScale({ steps }); }

  find(query: string, direction: "new" | "next" | "previous" = "new"): void {
    this.events.dispatch("find", {
      source: this, type: direction === "new" ? "" : "again", query,
      caseSensitive: false, entireWord: false, highlightAll: true,
      findPrevious: direction === "previous", matchDiacritics: false,
    });
  }

  async open(bytes: Uint8Array, password?: string): Promise<PDFDocumentProxy> {
    const previous = this.#load;
    this.#reset();
    const assets = this.#options.assets;
    const task = getDocument({
      data: bytes.slice(), password, cMapUrl: assets.cmaps, standardFontDataUrl: assets.fonts,
      wasmUrl: assets.wasm, iccUrl: assets.iccs, enableXfa: false,
    });
    this.#load = task;
    await previous?.destroy();
    try {
      const pdf = await task.promise;
      const labels = await pdf.getPageLabels();
      if (task !== this.#load) throw new Error("打开操作已取消。");
      this.#pdf = pdf;
      this.#labels = labels;
      const initialized = new Promise<void>(resolve => {
        this.events.on("pagesinit", () => {
          this.#viewer.currentScaleValue = "page-width";
          this.#viewer.setPageLabels(this.#labels);
          this.#options.onPage?.(1, this.label(1));
          resolve();
        }, { once: true });
      });
      this.#viewer.setDocument(pdf);
      this.#link.setDocument(pdf);
      await initialized;
      return pdf;
    } catch (error) {
      if (task === this.#load) this.#reset();
      await task.destroy();
      throw error;
    }
  }

  async close(): Promise<void> {
    const task = this.#load;
    this.#reset();
    this.#load = null;
    await task?.destroy();
  }

  async destroy(): Promise<void> {
    this.#listeners.abort();
    await this.close();
  }

  #reset(): void {
    this.#native = null;
    this.#editorReady = false;
    this.#pendingStyle = null;
    this.#pdf = null;
    this.#labels = null;
    this.#originals.clear();
    this.#viewer.setDocument(null);
    this.#link.setDocument(null);
    this.#signature?.reject(new Error("签名操作已取消。"));
    this.#signature = null;
  }

  async select(tool: NativeTool): Promise<void> {
    if (!this.#native) throw new Error("请先打开 PDF。");
    const mode = MODES[tool];
    // 6.5 的 setter 接受对象，getter 返回当前模式数字。
    if ((this.#viewer.annotationEditorMode as unknown) === mode) return;
    await new Promise<void>(resolve => {
      const changed = (event: { mode: number }) => {
        if (event.mode !== mode) return;
        this.events.off("annotationeditormodechanged", changed);
        resolve();
      };
      this.events.on("annotationeditormodechanged", changed);
      this.#viewer.annotationEditorMode = { mode };
    });
  }

  setStyle(tool: NativeTool, style: NativeStyle): void {
    const native = this.#native;
    if (!native) return;
    if (!this.#editorReady) {
      this.#pendingStyle = { tool, style };
      return;
    }
    if (tool === "highlight") {
      if (style.color) native.updateParams(Params.HIGHLIGHT_COLOR, style.color);
    } else if (tool === "textBox") {
      if (style.color) native.updateParams(Params.FREETEXT_COLOR, style.color);
      if (style.fontSize !== undefined) native.updateParams(Params.FREETEXT_SIZE, style.fontSize);
    } else if (tool === "ink") {
      if (style.color) native.updateParams(Params.INK_COLOR, style.color);
      if (style.opacity !== undefined) native.updateParams(Params.INK_OPACITY, style.opacity);
      if (style.width !== undefined) native.updateParams(Params.INK_THICKNESS, style.width);
    }
  }

  insertText(): void { this.#native?.updateParams(Params.CREATE, {}); }
  deleteSelected(): void { this.#native?.delete(); }
  undo(): void { this.#native?.undo(); }
  redo(): void { this.#native?.redo(); }
  command(apply: () => void, undo: () => void): void {
    if (!this.#native) throw new Error("请先打开 PDF。");
    this.#native.addCommands({ cmd: apply, undo, mustExec: true });
  }

  setSelectedComment(text: string): boolean {
    const editor = (this.#native?.getActive() ?? this.#native?.firstSelectedEditor) as
      { comment: string | object | null } | null | undefined;
    if (!editor) return false;
    const previous = editor.comment;
    this.command(() => {
      editor.comment = text || null;
      this.#options.onChange?.();
    }, () => {
      editor.comment = previous;
      this.#options.onChange?.();
    });
    return true;
  }

  async insertImage(file: File): Promise<void> {
    await this.select("image");
    this.#native!.updateParams(Params.CREATE, { bitmapFile: file });
  }

  async insertSignature(input: SignatureInput): Promise<void> {
    await this.select("signature");
    if (this.#signature) throw new Error("正在创建签名。");
    await new Promise<void>((resolve, reject) => {
      this.#signature = { input, resolve, reject };
      try {
        this.#native!.updateParams(Params.CREATE, {});
      } catch (error) {
        this.#signature = null;
        reject(error);
      }
    });
  }

  async #placeSignature(editor: SignatureEditor): Promise<void> {
    const pending = this.#signature;
    if (!pending) { editor.remove(); return; }
    const { input } = pending;
    try {
      let data: unknown;
      if (input.kind === "text") {
        data = editor.getFromText(input.text, { fontFamily: input.fontFamily ?? "serif", fontStyle: "italic", fontWeight: "normal" });
      } else if (input.kind === "draw") {
        data = editor.getDrawnSignature({
          curves: input.strokes.map(stroke => ({ points: stroke.flatMap(({ x, y }) => [x, y]) })),
          width: input.width, height: input.height, thickness: input.thickness,
        });
      } else {
        const bitmap = await createImageBitmap(input.file);
        try { data = await editor.getFromImage(bitmap); } finally { bitmap.close(); }
      }
      if (pending !== this.#signature) { editor.remove(); return; }
      if (!data) throw new Error("签名没有可绘制的内容。");
      editor.addSignature(data, 40, "签名");
      pending.resolve();
    } catch (error) {
      editor.remove();
      pending.reject(error);
    } finally {
      if (this.#signature === pending) this.#signature = null;
    }
  }

  async capture(fields: readonly Field[]): Promise<Pick<Plan, "add" | "remove" | "fill">> {
    const pdf = this.#pdf;
    if (!pdf) throw new Error("请先打开 PDF。");
    this.#native?.commitOrRemove();
    const map: ReadonlyMap<string, unknown> = pdf.annotationStorage.serializable.map ?? new Map();
    const edits = await captureEditors(map, { resolveExisting: (id, page) => this.#findOriginal(id, page) });
    const fill = fields.length ? captureFields(fields, await pdf.getFieldObjects(), map) : [];
    return { ...edits, fill };
  }

  async #findOriginal(id: string, page: number): Promise<ReadAnnotation | undefined> {
    const direct = (await this.#lookup(page)).get(id);
    if (direct) return direct;
    for (let other = 1; other <= (this.#pdf?.numPages ?? 0); other++) {
      if (other === page) continue;
      const found = (await this.#lookup(other)).get(id);
      if (found) return found;
    }
    return undefined;
  }

  #lookup(page: number): Promise<ReadonlyMap<string, ReadAnnotation>> {
    let result = this.#originals.get(page);
    if (!result) {
      result = this.#matchAnnotations(page);
      this.#originals.set(page, result);
    }
    return result;
  }

  async #matchAnnotations(page: number): Promise<ReadonlyMap<string, ReadAnnotation>> {
    const pdf = this.#pdf;
    if (!pdf || !this.#options.annotations) return new Map();
    const original = await this.#options.annotations(page);
    const native = (await (await pdf.getPage(page)).getAnnotations({ intent: "any" }) as {
      id: string; subtype: string; rect?: number[];
    }[]).filter(annotation => !["Popup", "Widget"].includes(annotation.subtype));
    const kinds: Record<string, ReadAnnotation["kind"]> = {
      Highlight: "highlight", Underline: "underline", StrikeOut: "strikeout", Text: "note",
      FreeText: "textBox", Ink: "ink", Square: "rectangle", Circle: "ellipse", Stamp: "image", Link: "link",
    };
    const matchesType = (entry: (typeof native)[number], mark: ReadAnnotation) =>
      entry.subtype === "Line" ? mark.kind === "line" || mark.kind === "arrow" :
        mark.kind === "other" ? mark.subtype === entry.subtype : kinds[entry.subtype] === mark.kind;
    const aligned = native.length === original.length && native.every((entry, i) => matchesType(entry, original[i]));
    const map = new Map<string, ReadAnnotation>();
    native.forEach((entry, i) => {
      const rect = entry.rect;
      if (!rect || rect.length !== 4) return;
      const candidates = original.filter(mark => matchesType(entry, mark) &&
        [mark.rect.x, mark.rect.y, mark.rect.x + mark.rect.width, mark.rect.y + mark.rect.height]
          .every((value, index) => Math.abs(value - rect[index]) < 1));
      if (candidates.length === 1) map.set(entry.id, candidates[0]);
      else if (aligned && candidates.includes(original[i])) map.set(entry.id, original[i]);
    });
    return map;
  }
}

