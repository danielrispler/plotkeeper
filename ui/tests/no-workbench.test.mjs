// The page holds only what helps Daniel understand (danielrispler/plotkeeper#326): no terminal, project, branch, skill,
// usage, settings-page or restart code, no drag-drop and no window drag region, and every command the page calls is one the
// server serves.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const sources = readdirSync(new URL("../", import.meta.url))
  .filter((name) => name.endsWith(".ts"))
  .map((name) => [name, read(`../${name}`)]);
const html = read("../index.html");
const served = new Set([...read("../../server/src/api.rs").matchAll(/^ {8}"([a-z_]+)" =>/gm)].map((m) => m[1]));

test("every command the page calls is served", () => {
  const unserved = [];
  for (const [name, source] of sources) {
    for (const [, command] of source.matchAll(/\binvoke(?:<[^(]*>)?\(\s*"([a-z_]+)"/g)) {
      if (!served.has(command)) unserved.push(`${name}: ${command}`);
    }
  }
  assert.deepEqual(unserved, []);
});

test("no workbench module, drag-drop or drag region remains", () => {
  const names = sources.map(([name]) => name);
  for (const gone of ["terminal", "panel", "daemon", "restart", "picker", "workspaces", "skills", "usage", "settings-page"]) {
    assert.ok(!names.includes(`${gone}.ts`), `${gone}.ts`);
  }
  const text = sources.map(([, source]) => source).join("\n") + html;
  for (const word of ["tauri://drag-drop", "data-tauri-drag-region", "xterm", "terminal-body", "settings-dialog"]) {
    assert.ok(!text.includes(word), word);
  }
});

test("every element the page looks up is in the page", () => {
  const ids = new Set([...html.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1]));
  const missing = [];
  for (const [name, source] of sources) {
    for (const [, id] of source.matchAll(/\belement(?:<[^(]*>)?\(\s*"([^"]+)"/g)) {
      if (!ids.has(id)) missing.push(`${name}: ${id}`);
    }
  }
  assert.deepEqual(missing, []);
});
