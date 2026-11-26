import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { captureEditors } from "../dist/capture.js";
import { captureFields } from "../dist/forms.js";
import { PageOrganizer } from "../dist/organizer.js";
import { OutlineEditor } from "../dist/outline.js";
import { compactPlan } from "../dist/plan.js";

const fixture = name => JSON.parse(readFileSync(new URL(`../../tests/fixtures/plan/${name}.json`, import.meta.url)));

test("页面、目录及链接用同一套结构改动单", () => {
  const pages = new PageOrganizer(2);
  assert.equal(pages.plan(), undefined);
  pages.remove(0);
  pages.duplicate(0);
  pages.rotate(0);
  assert.equal(pages.insert(1, 1), 1);

  const outline = new OutlineEditor([]);
  outline.insert([], { title: "Part I", target: { kind: "none" } });
  outline.insert([0], { title: "Chapter", target: { kind: "page", page: 2, y: 330 } });
  outline.rename([0, 0], "Chapter 1");
  outline.remap(page => page + 1);

  const plan = compactPlan({
    pages: pages.plan(), outline: outline.plan(),
    add: [
      { source: 0, page: 2, kind: "link", rect: { x: 20, y: 300, width: 90, height: 14 }, target: { kind: "page", page: 2, y: 310 } },
      { source: 1, page: 1, kind: "link", rect: { x: 20, y: 280, width: 90, height: 14 }, target: { kind: "url", url: "https://example.com/reference" } },
    ],
    info: { title: "Edition one", creator: "engine-pdf", keywords: ["draft", "pdf"] },
  });
  assert.deepEqual(plan, fixture("structure"));
  outline.remove([0]);
  assert.deepEqual(compactPlan({ outline: outline.plan() }), { outline: [] });
});

test("原生标注序列化与控件状态生成同一套标注、填表改动单", async () => {
  const storage = new Map([
    ["editor0", { id: "10R", annotationType: 9, pageIndex: 0, rect: [20, 90, 100, 102],
      quadPoints: new Float32Array([20, 102, 100, 102, 20, 90, 100, 90]),
      color: [255, 255, 0], opacity: 0.5, popup: { contents: "Check this." } }],
    ["editor1", { annotationType: 3, pageIndex: 0, rect: [30, 40, 150, 76], value: "Revision", fontSize: 12 }],
    ["text", { value: "A. Lee" }],
    ["checkbox", { value: true }],
  ]);
  const captured = await captureEditors(storage, {
    resolveExisting: (id, page) => {
      assert.equal(id, "10R");
      assert.equal(page, 1);
      return { id: { source: 0, page: 1, index: 0 }, author: "Reviewer", kind: "highlight" };
    },
  });
  const fill = captureFields([
    { name: "name", kind: "text", value: "Before" },
    { name: "agree", kind: "checkbox", value: false },
  ], new Map([["name", [{ id: "text" }]], ["agree", [{ id: "checkbox", exportValues: "Yes" }]]]), storage);
  assert.deepEqual(compactPlan({ ...captured, fill }), fixture("annotations"));
});
