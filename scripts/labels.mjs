import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const files = process.argv.slice(2).map(file => resolve(file));
if (!files.length) {
  console.error("用法：node scripts/labels.mjs <PDF> [<PDF> ...]。可用 PDFJS_MODULE 指定 PDF.js 6.5 的 legacy/build/pdf.mjs。");
  process.exit(1);
}

const { getDocument, version } = await import(process.env.PDFJS_MODULE
  ? pathToFileURL(resolve(process.env.PDFJS_MODULE)).href
  : "pdfjs-dist/legacy/build/pdf.mjs");
if (!version.startsWith("6.5.")) throw new Error(`需要 PDF.js 6.5.x，当前是 ${version}`);

const cwd = fileURLToPath(new URL("../", import.meta.url));
const target = execFileSync("cargo", ["metadata", "--no-deps", "--format-version", "1"], { cwd });
const probe = resolve(JSON.parse(target).target_directory, "debug/examples/probe");
execFileSync("cargo", ["build", "--quiet", "--example", "probe"], { cwd, stdio: "inherit" });

let pages = 0;
let differences = 0;
let unlabeled = 0;
let errors = 0;
for (const file of files) {
  try {
    const task = getDocument({ data: new Uint8Array(await readFile(file)), verbosity: 0 });
    try {
      const pdf = await task.promise;
      const jsLabels = await pdf.getPageLabels();
      const engineLabels = JSON.parse(execFileSync(probe, [file, "--labels"], { encoding: "utf8" }));
      if (pdf.numPages !== engineLabels.length) throw new Error("两边页数不同");
      pages += pdf.numPages;
      if (jsLabels === null) unlabeled++;
      const mismatches = engineLabels.flatMap((label, index) => {
        const expected = jsLabels === null ? null : jsLabels[index];
        return label === expected ? [] : [{ page: index + 1, engine: label, pdfjs: expected }];
      });
      differences += mismatches.length;
      if (mismatches.length) console.log(file, mismatches.length, JSON.stringify(mismatches.slice(0, 5)));
    } finally {
      await task.destroy();
    }
  } catch (error) {
    errors++;
    console.error(file, error.message);
  }
}
console.log(`PDF.js ${version} · ${files.length - errors} 份 ${pages} 页 · ${unlabeled} 份没有标签 · ${differences} 页有差异 · ${errors} 份读取失败`);
if (differences || errors) process.exitCode = 1;
