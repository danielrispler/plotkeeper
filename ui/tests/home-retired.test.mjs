// Home is retired (spec #380, ticket #388): the map is the page's base, and what Home held lives in the session list. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { tauri } from "./dom.mjs";

tauri([], () => null);
const list = await import("../../dist/list.js");
const panes = await import("../../dist/panes.js");
const layout = await import("../../dist/layout.js");

const ui = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, ui), "utf8");

test("Home's module, its view and its styles are gone, and no script names it", () => {
  assert.equal(fs.existsSync(new URL("home.ts", ui)), false);
  for (const name of ["showHome", "homeShown", "refreshHome", "redrawHome"]) assert.equal(name in panes, false, name);
  assert.equal("homeSeen" in layout, false);
  assert.doesNotMatch(read("seen.ts"), /homeShown/);
  assert.doesNotMatch(read("style.css"), /\.pane\.home|\.home-row/);
  const scripts = fs.readdirSync(ui).filter((name) => name.endsWith(".ts"));
  assert.deepEqual(scripts.filter((name) => /from "\.\/home\.js"/.test(read(name))), []);
});

test("the list owns what Home lent it: a click on its empty space and Clear landed", () => {
  assert.equal(typeof list.listSpace, "function");
  assert.equal(typeof list.clearing, "function");
});
