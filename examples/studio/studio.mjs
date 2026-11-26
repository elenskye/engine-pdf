import { getDocument } from "pdfjs-dist";
import { EditorViewer, EditorTools, OutlineEditor, PageOrganizer, compactPlan, renderThumbnail } from "/dist/index.js";

const element = id => document.getElementById(id);
const assets = {
  worker: "/pdfjs/build/pdf.worker.mjs", images: "/pdfjs/web/images/",
  cmaps: "/pdfjs/cmaps/", fonts: "/pdfjs/standard_fonts/", wasm: "/pdfjs/wasm/", iccs: "/pdfjs/iccs/",
};
let sources = [];
let meta, organizer, outline, selectedOutline, downloadUrl;
let selectedPage = 0;
let busy = false;
const infoFields = ["title", "author", "subject", "keywords", "creator"];
const annotations = new Map();
const reader = new EditorViewer({
  container: element("viewer-container"), viewer: element("viewer"), assets,
  annotations: readerAnnotations,
  onPage(page, label) {
    element("page-number").value = page;
    element("page-number").title = label ?? "";
    element("page-count").textContent = `/ ${reader.document?.numPages ?? 0}${label ? ` · ${label}` : ""}`;
    void showAnnotations();
  },
  onScale: scale => { element("scale").value = `${Math.round(scale * 100)}%`; },
  onFind: (current, total) => { element("find-count").value = element("search").value ? `${current} / ${total}` : ""; },
});
const tools = new EditorTools(reader, () => { void showAnnotations(); });

function status(message, error = false) {
  element("status").textContent = message;
  element("status").dataset.error = String(error);
}

async function request(path, files, extra = {}) {
  const header = new TextEncoder().encode(JSON.stringify({
    lengths: files.map(file => file.bytes.length), passwords: files.map(file => file.password ?? null), ...extra,
  }));
  const prefix = new Uint8Array(4);
  new DataView(prefix.buffer).setUint32(0, header.length, true);
  const response = await fetch(`/api/${path}`, {
    method: "POST", headers: { "Content-Type": "application/x-engine-pdf" },
    body: new Blob([prefix, header, ...files.map(file => file.bytes)]),
  });
  if (!response.ok) throw new Error(await response.text() || `请求失败（${response.status}）。`);
  return response;
}

async function readerAnnotations(page) {
  if (!sources[0]) return [];
  if (!annotations.has(page)) {
    const result = request("annotations", [sources[0]], { page }).then(response => response.json());
    annotations.set(page, result);
    result.catch(() => {
      if (annotations.get(page) === result) annotations.delete(page);
    });
  }
  return annotations.get(page);
}

function controls() {
  const enabled = !!organizer && !busy;
  for (const control of document.querySelectorAll(".toolbar button, .toolbar input, .toolbar select, .sidebar button, .sidebar input, .sidebar select, .navigation button, .navigation input")) {
    control.disabled = !enabled;
  }
  element("save").disabled = !enabled;
  element("open").disabled = busy;
  if (!enabled) return;
  element("page-up").disabled = selectedPage === 0;
  element("page-down").disabled = selectedPage === organizer.pages.length - 1;
  element("page-remove").disabled = organizer.pages.length === 1;
  element("font-size").disabled = tools.tool !== "textBox";
  element("opacity").disabled = !["ink", "underline", "strikeout", "note", "rectangle", "ellipse", "line", "arrow"].includes(tools.tool);
  element("width").disabled = !["ink", "rectangle", "ellipse", "line", "arrow"].includes(tools.tool);
  element("source-password").hidden = !sources[organizer.pages[selectedPage].source].encrypted;
  for (const id of ["child", "rename", "remove", "up", "down", "promote", "indent"]) {
    element(`outline-${id}`).disabled = !selectedOutline;
  }
  for (const remove of element("annotations").querySelectorAll("button[data-pending='true']")) remove.disabled = true;
}

async function work(action) {
  if (busy) return;
  busy = true;
  controls();
  try {
    await action();
  } catch (error) {
    status(error.message, true);
  } finally {
    busy = false;
    controls();
  }
}

function askPassword(title = "打开加密 PDF") {
  const dialog = element("unlock-dialog");
  const input = element("open-password");
  dialog.returnValue = "";
  element("unlock-title").textContent = title;
  input.value = "";
  dialog.showModal();
  return new Promise(resolve => dialog.addEventListener("close", () => {
    resolve(dialog.returnValue === "open" ? input.value : undefined);
    input.value = "";
  }, { once: true }));
}

async function inspect(source) {
  for (;;) {
    try {
      return await (await request("inspect", [source])).json();
    } catch (error) {
      if (!/加了密码|密码不正确/.test(error.message)) throw error;
      status(error.message);
      source.password = await askPassword();
      if (source.password === undefined) return null;
    }
  }
}

async function open(source, info) {
  const previous = sources;
  sources = [];
  annotations.clear();
  try {
    source.pdf = await reader.open(source.bytes, source.password);
  } catch (error) {
    organizer = undefined;
    element("empty").hidden = false;
    element("document-name").textContent = "尚未打开 PDF";
    element("pages").replaceChildren();
    element("outline").replaceChildren();
    element("annotations").replaceChildren();
    throw error;
  } finally {
    await Promise.allSettled(previous.map(item => item.task?.destroy()));
  }
  sources = [source];
  source.encrypted = info.info.encrypted;
  meta = info;
  organizer = new PageOrganizer(meta.pageCount);
  outline = new OutlineEditor(meta.outline);
  selectedPage = 0;
  selectedOutline = undefined;
  tools.reset();
  showTool();
  element("empty").hidden = true;
  element("document-name").textContent = source.name;
  for (const key of infoFields) element(key).value = key === "keywords" ? (meta.info.keywords ?? []).join(", ") : (meta.info[key] ?? "");
  element("search").value = "";
  element("find-count").value = "";
  element("flatten").checked = false;
  element("security").value = "none";
  element("lock-options").hidden = true;
  element("owner-password").value = element("user-password").value = "";
  element("page-number").max = meta.pageCount;
  showPages();
  showOutline();
  void showAnnotations();
  status(meta.formError ?? (meta.info.encrypted ? "已打开加密 PDF。编辑时需要权限密码和输出密码选项。" : ""));
}

function fileAction(id, action) {
  element(id).addEventListener("change", event => {
    const file = event.target.files[0];
    event.target.value = "";
    if (file) void work(() => action(file));
  });
}

fileAction("open", async file => {
  const source = { bytes: new Uint8Array(await file.arrayBuffer()), name: file.name };
  const info = await inspect(source);
  if (info) {
    await open(source, info);
    element("download").hidden = true;
    if (downloadUrl) URL.revokeObjectURL(downloadUrl);
  }
});

fileAction("insert", async file => {
  const source = { bytes: new Uint8Array(await file.arrayBuffer()), name: file.name };
  const info = await inspect(source);
  if (!info) return;
  const task = getDocument({
    data: source.bytes.slice(), password: source.password, cMapUrl: assets.cmaps,
    standardFontDataUrl: assets.fonts, wasmUrl: assets.wasm, iccUrl: assets.iccs, enableXfa: false,
  });
  try {
    source.pdf = await task.promise;
  } catch (error) {
    await task.destroy();
    throw error;
  }
  source.task = task;
  source.encrypted = info.info.encrypted;
  changePages(() => {
    sources[organizer.insert(info.pageCount, selectedPage + 1)] = source;
    selectedPage++;
  });
  status(`已插入 ${info.pageCount} 页。保存后可编辑插入的页面。`);
});

const thumbs = new IntersectionObserver(entries => {
  const pages = organizer?.pages;
  for (const { target, isIntersecting } of entries) {
    if (!isIntersecting) continue;
    thumbs.unobserve(target);
    const pick = pages?.[Number(target.dataset.index)];
    if (pick && target.isConnected) {
      void renderThumbnail(sources[pick.source].pdf, pick, target).catch(() => {});
    }
  }
}, { root: document.querySelector(".sidebar") });

function showPages() {
  thumbs.disconnect();
  element("pages").replaceChildren(...organizer.pages.map((pick, index) => {
    const item = document.createElement("li");
    item.dataset.index = index;
    item.draggable = true;
    const button = document.createElement("button");
    button.setAttribute("aria-current", String(index === selectedPage));
    button.title = `${sources[pick.source].name} · 原第 ${pick.page} 页 · 旋转 ${pick.rotate}°`;
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 80;
    canvas.dataset.index = index;
    const label = document.createElement("span");
    label.textContent = `${index + 1} · ${pick.source ? "插入" : "原"} ${pick.page}${pick.rotate ? ` ↻ ${pick.rotate}°` : ""}`;
    button.append(canvas, label);
    button.addEventListener("click", () => {
      selectedPage = index;
      element("pages").querySelector("[aria-current='true']")?.setAttribute("aria-current", "false");
      button.setAttribute("aria-current", "true");
      controls();
      if (pick.source === 0) reader.goToPage(pick.page);
      else status("插入页已加入输出页序。保存后可在编辑区查看。");
    });
    item.append(button);
    thumbs.observe(canvas);
    return item;
  }));
}

function changePages(action) {
  const before = organizer.pages;
  try {
    action();
    const after = organizer.pages;
    outline.remap(page => {
      const old = before[page - 1];
      const index = old ? after.findIndex(pick => pick.source === old.source && pick.page === old.page) : -1;
      return index < 0 ? undefined : index + 1;
    });
    showPages();
    showOutline();
    controls();
  } catch (error) {
    status(error.message, true);
  }
}

let draggingPage;
element("pages").addEventListener("dragstart", event => {
  const item = event.target.closest("li");
  if (busy || !item) return event.preventDefault();
  draggingPage = Number(item.dataset.index);
  event.dataTransfer.setData("text/plain", String(draggingPage + 1));
  event.dataTransfer.effectAllowed = "move";
});
element("pages").addEventListener("dragover", event => {
  if (draggingPage === undefined || !event.target.closest("li")) return;
  event.preventDefault();
  element("pages").querySelector("[data-drop]")?.removeAttribute("data-drop");
  event.target.closest("li").dataset.drop = "true";
});
element("pages").addEventListener("dragend", () => {
  draggingPage = undefined;
  element("pages").querySelector("[data-drop]")?.removeAttribute("data-drop");
});
element("pages").addEventListener("drop", event => {
  const item = event.target.closest("li");
  if (draggingPage === undefined || !item || busy) return;
  event.preventDefault();
  changePages(() => { organizer.move(draggingPage, Number(item.dataset.index)); selectedPage = Number(item.dataset.index); });
  draggingPage = undefined;
});
element("page-up").onclick = () => changePages(() => { organizer.move(selectedPage, selectedPage - 1); selectedPage--; });
element("page-down").onclick = () => changePages(() => { organizer.move(selectedPage, selectedPage + 1); selectedPage++; });
element("page-rotate").onclick = () => changePages(() => organizer.rotate(selectedPage));
element("page-duplicate").onclick = () => changePages(() => { organizer.duplicate(selectedPage); selectedPage++; });
element("page-remove").onclick = () => changePages(() => { organizer.remove(selectedPage); selectedPage = Math.min(selectedPage, organizer.pages.length - 1); });

function showOutline(items = outline.items, parent = []) {
  const list = document.createElement("ul");
  items.forEach((item, index) => {
    const path = [...parent, index];
    const row = document.createElement("li");
    const label = document.createElement("button");
    const page = item.target.kind === "page" ? item.target.page : null;
    label.textContent = page ? `${item.title} · ${page}` : item.title;
    label.setAttribute("aria-current", String(path.join("/") === selectedOutline?.join("/")));
    label.onclick = () => {
      selectedOutline = path;
      element("outline-title").value = item.title;
      if (page && organizer.pages[page - 1]) element("pages").querySelector(`[data-index='${page - 1}'] button`)?.click();
      showOutline();
      controls();
    };
    row.append(label);
    if (item.children?.length) row.append(showOutline(item.children, path));
    list.append(row);
  });
  if (!parent.length) element("outline").replaceChildren(list);
  return list;
}

for (const action of ["add", "child", "rename", "remove", "up", "down", "promote", "indent"]) {
  element(`outline-${action}`).onclick = async () => {
    try {
      const title = element("outline-title").value.trim();
      const path = selectedOutline ?? [];
      const parent = path.slice(0, -1);
      const index = path.at(-1);
      if (["add", "child", "rename"].includes(action) && !title) throw new Error("请输入目录标题。");
      // 目录指向所选页的顶端（PDF 坐标原点在左下，y 取裁切框上沿）。
      const pick = organizer.pages[selectedPage];
      const top = (await sources[pick.source].pdf.getPage(pick.page)).view[3];
      const item = { title, target: { kind: "page", page: selectedPage + 1, y: top } };
      switch (action) {
        case "add": outline.insert([], item); break;
        case "child": outline.insert(path, item); break;
        case "rename": outline.rename(path, title); break;
        case "remove": outline.remove(path); selectedOutline = undefined; break;
        case "up": case "down": {
          const next = index + (action === "up" ? -1 : 1);
          outline.move(path, parent, next);
          selectedOutline = [...parent, next];
          break;
        }
        case "promote": {
          if (!parent.length) return;
          const next = parent.at(-1) + 1;
          outline.move(path, parent.slice(0, -1), next);
          selectedOutline = [...parent.slice(0, -1), next];
          break;
        }
        case "indent": {
          if (index === 0) return;
          const destination = [...parent, index - 1];
          const previous = destination.reduce((items, i, level) => level === destination.length - 1 ? items[i] : items[i].children, outline.items);
          const next = previous.children?.length ?? 0;
          outline.move(path, destination, next);
          selectedOutline = [...destination, next];
          break;
        }
      }
      showOutline();
      controls();
    } catch (error) {
      status(error.message, true);
    }
  };
}

const markNames = {
  highlight: "高亮", underline: "下划线", strikeout: "删除线", note: "便签", textBox: "文本框", ink: "绘图",
  rectangle: "矩形", ellipse: "椭圆", line: "直线", arrow: "箭头", image: "图章", link: "链接", other: "其他标注",
};
async function showAnnotations() {
  if (!element("annotations-panel").open || !sources[0]) return;
  const source = sources[0];
  const page = reader.pageNumber;
  try {
    const marks = await readerAnnotations(page);
    if (source !== sources[0] || page !== reader.pageNumber) return;
    element("annotations").replaceChildren(...marks.map(mark => {
      const row = document.createElement("li");
      const label = document.createElement("span");
      label.textContent = `${markNames[mark.kind]}${mark.contents ? ` · ${mark.contents}` : ""}`;
      const remove = document.createElement("button");
      const pending = tools.remove.some(id => id.page === page && id.index === mark.id.index);
      remove.textContent = pending ? "待删除" : "删除";
      remove.dataset.pending = String(pending);
      remove.disabled = pending || busy;
      remove.onclick = () => tools.removeMark(mark.id);
      row.append(label, remove);
      return row;
    }));
  } catch (error) {
    if (source === sources[0]) status(error.message, true);
  }
}

element("annotations-panel").addEventListener("toggle", () => { void showAnnotations(); });

function style() {
  const rgb = Number.parseInt(element("color").value.slice(1), 16);
  const page = Number(element("link-target").value);
  const url = element("link-target").value.trim();
  const target = element("link-kind").value === "page"
    ? (Number.isInteger(page) && page >= 1 ? { kind: "page", page, y: Number(element("link-y").value) } : undefined)
    : (url ? { kind: "url", url } : undefined);
  tools.setStyle({
    color: { red: (rgb >> 16) / 255, green: ((rgb >> 8) & 255) / 255, blue: (rgb & 255) / 255, alpha: Number(element("opacity").value) },
    width: Number(element("width").value) || 2, fontSize: Number(element("font-size").value) || 14,
    contents: element("note-text").value, target,
  });
}

function showTool(tool = tools.tool) {
  for (const button of element("tools").querySelectorAll("[data-tool]")) button.setAttribute("aria-pressed", String(button.dataset.tool === tool));
  element("add-image").setAttribute("aria-pressed", String(tool === "image"));
  element("add-signature").setAttribute("aria-pressed", String(tool === "signature"));
  element("viewer-container").dataset.draw = String(["note", "rectangle", "ellipse", "line", "arrow", "link"].includes(tool));
  element("note-options").hidden = !["note", "highlight", "textBox", "ink", "image", "signature"].includes(tool);
  element("apply-comment").hidden = tool === "note";
  element("note-text").placeholder = tool === "note" ? "输入后点击页面" : "附到当前选中的标注";
  element("link-options").hidden = tool !== "link";
}

for (const button of element("tools").querySelectorAll("[data-tool]")) {
  const tool = button.dataset.tool;
  const markup = tool === "underline" || tool === "strikeout";
  if (markup) button.addEventListener("pointerdown", event => event.preventDefault());
  button.onclick = () => void work(async () => {
    await tools.select(tool);
    showTool();
    style();
    if (markup) tools.markSelection(tool);
  });
}
for (const id of ["color", "opacity", "width", "font-size", "note-text", "link-kind", "link-target", "link-y"]) {
  element(id).addEventListener("input", style);
}
element("viewer-container").addEventListener("pointerup", () => {
  if (tools.tool === "underline" || tools.tool === "strikeout") tools.markSelection(tools.tool);
});
element("undo").onclick = () => reader.undo();
element("redo").onclick = () => reader.redo();
element("delete-selected").onclick = () => reader.deleteSelected();
element("apply-comment").onclick = () => {
  if (!reader.setSelectedComment(element("note-text").value)) status("请先选中一个可编辑的标注。", true);
};
element("add-image").onclick = () => element("image-file").click();
fileAction("image-file", async file => { await tools.select("image"); await reader.insertImage(file); showTool(); });

const pad = element("signature-pad");
const pen = pad.getContext("2d");
let signatureStrokes = [];
let signaturePointer;
element("clear-signature").onclick = () => { signatureStrokes = []; pen.clearRect(0, 0, pad.width, pad.height); };
pad.addEventListener("pointerdown", event => {
  if (signaturePointer !== undefined) return;
  signaturePointer = event.pointerId;
  pad.setPointerCapture(signaturePointer);
  signatureStrokes.push([]);
});
pad.addEventListener("pointermove", event => {
  if (signaturePointer !== event.pointerId) return;
  const box = pad.getBoundingClientRect();
  const point = { x: (event.clientX - box.left) * pad.width / box.width, y: (event.clientY - box.top) * pad.height / box.height };
  const stroke = signatureStrokes.at(-1);
  const last = stroke.at(-1);
  stroke.push(point);
  if (last) {
    pen.lineWidth = 2;
    pen.lineCap = "round";
    pen.beginPath();
    pen.moveTo(last.x, last.y);
    pen.lineTo(point.x, point.y);
    pen.stroke();
  }
});
for (const event of ["pointerup", "pointercancel", "lostpointercapture"]) pad.addEventListener(event, () => { signaturePointer = undefined; });
element("signature-kind").onchange = () => {
  const kind = element("signature-kind").value;
  for (const type of ["text", "draw", "image"]) element(`signature-${type}-panel`).hidden = type !== kind;
};
element("add-signature").onclick = () => {
  const dialog = element("signature-dialog");
  dialog.returnValue = "";
  dialog.showModal();
};
element("signature-dialog").addEventListener("close", event => {
  if (event.target.returnValue !== "insert") return;
  const kind = element("signature-kind").value;
  const text = element("signature-text").value.trim();
  const file = element("signature-file").files[0];
  const strokes = signatureStrokes.filter(stroke => stroke.length > 1);
  const input = kind === "text" ? (text && { kind, text }) : kind === "image" ? (file && { kind, file })
    : (strokes.length && { kind, strokes, width: pad.width, height: pad.height, thickness: 2 });
  if (!input) return status("请先输入、绘制或选取签名。", true);
  void work(async () => { await tools.select("signature"); await reader.insertSignature(input); showTool(); });
});

element("previous-page").onclick = () => { if (reader.pageNumber > 1) reader.goToPage(reader.pageNumber - 1); };
element("next-page").onclick = () => { if (reader.pageNumber < meta.pageCount) reader.goToPage(reader.pageNumber + 1); };
element("page-number").onchange = () => {
  try { reader.goToPage(Number(element("page-number").value)); }
  catch (error) { status(error.message, true); element("page-number").value = reader.pageNumber; }
};
element("zoom-out").onclick = () => reader.zoomBy(-1);
element("zoom-in").onclick = () => reader.zoomBy(1);
element("fit-width").onclick = () => reader.zoom("page-width");
element("search").oninput = () => reader.find(element("search").value);
element("find-previous").onclick = () => reader.find(element("search").value, "previous");
element("find-next").onclick = () => reader.find(element("search").value, "next");
element("security").onchange = () => { element("lock-options").hidden = element("security").value !== "lock"; };
element("source-password").onclick = () => void work(async () => {
  const source = sources[organizer.pages[selectedPage].source];
  const password = await askPassword(`输入权限密码 · ${source.name}`);
  if (password === undefined) return;
  await request("inspect", [{ bytes: source.bytes, password }]);
  source.password = password;
  status("已更新所选来源的密码。");
});

element("save").onclick = () => void work(async () => {
  status("正在保存…");
  const native = await reader.capture(meta.fields);
  const used = new Set(organizer.pages.map(({ source, page }) => `${source}:${page}`));
  const marks = [...native.add, ...tools.add].filter(mark => used.has(`${mark.source}:${mark.page}`));
  const remove = [...new Map([...native.remove, ...tools.remove].map(id => [`${id.source}:${id.page}:${id.index}`, id])).values()];
  const info = Object.fromEntries(infoFields.map(key => [key, element(key).value])
    .filter(([key, value]) => value !== (key === "keywords" ? (meta.info.keywords ?? []).join(", ") : (meta.info[key] ?? "")))
    .map(([key, value]) => [key, key === "keywords" ? value.split(",").map(word => word.trim()).filter(Boolean) : value]));
  const plan = compactPlan({ pages: organizer.plan(), outline: outline.plan(), add: marks, remove, fill: native.fill,
    info: Object.keys(info).length ? info : undefined });
  const security = element("security").value;
  const save = { flatten: element("flatten").checked, removePassword: security === "remove" };
  if (security === "lock") {
    if (!element("owner-password").value) throw new Error("请填写新权限密码。");
    save.lock = { owner: element("owner-password").value, user: element("user-password").value || null,
      permissions: Object.fromEntries(["print", "copy", "modify", "annotate"].map(key => [key, element(`allow-${key}`).checked])) };
  }
  const response = await request("save", sources, { plan, save });
  const blob = await response.blob();
  const name = response.headers.get("X-Output-File");
  if (downloadUrl) URL.revokeObjectURL(downloadUrl);
  downloadUrl = URL.createObjectURL(blob);
  Object.assign(element("download"), { href: downloadUrl, download: name, hidden: false });
  const source = { bytes: new Uint8Array(await blob.arrayBuffer()), name,
    password: save.lock?.owner ?? (save.removePassword ? undefined : sources[0].password) };
  try {
    const info = await inspect(source);
    if (info) await open(source, info);
  } catch (error) {
    status(`已保存 ${name}，自动重开失败。${error.message}`, true);
    return;
  }
  status(`已保存 ${name}`);
});

void work(async () => {
  const response = await fetch("/api/source");
  if (response.status === 204) return;
  if (!response.ok) throw new Error("无法读取启动时指定的 PDF。");
  const source = { bytes: new Uint8Array(await response.arrayBuffer()), name: "启动时打开的 PDF" };
  const info = await inspect(source);
  if (info) await open(source, info);
});
