// The page's wiring (danielrispler/plotkeeper#326): every command the page calls is one the server serves, and every element
// it looks up is in the page. Kept from `no-workbench.test.mjs` when its guard of the retired workbench went to `archive/` (#415).
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
