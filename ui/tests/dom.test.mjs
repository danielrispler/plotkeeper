// The page's DOM helpers: `patch` keeps nodes by key, and `newest` drops a slower, older reply. Moved from Home's tests (#388).
// Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { Field, fakeDocument, tauri } from "./dom.mjs";

tauri([]);
globalThis.document = fakeDocument({ visibilityState: "visible", hasFocus: () => true }, Field);

test("the newest request wins: a slower, older reply is dropped", async () => {
  const { newest } = await import("../../dist/dom.js");
  const take = newest();
  let first, second;
  const a = take(new Promise((r) => (first = r)));
  const b = take(new Promise((r) => (second = r)));
  second("newer");
  first("older");
  assert.deepEqual([await a, await b], [undefined, "newer"]);
});

test("patching by key keeps the slot's card when rows above it change (C11)", async () => {
  const { patch } = await import("../../dist/dom.js");
  const keyed = (tag, key, text) => Object.assign(new Field(tag, {}, text ? [text] : []), { attrs: new Map([["data-key", key]]) });
  const parent = new Field("div");
  const [a, b, slot] = [keyed("button", "a", "a"), keyed("button", "b", "b"), keyed("div", "slot", "card")];
  parent.childNodes = [a, b, slot];
  // A row above is dropped.
  patch(parent, [keyed("button", "a", "a"), keyed("div", "slot", "card")]);
  assert.deepEqual(parent.childNodes.map((n) => n.getAttribute("data-key")), ["a", "slot"]);
  assert.equal(parent.childNodes[1], slot);
  // A row is added above.
  patch(parent, [keyed("button", "a", "a"), keyed("button", "c", "c"), keyed("div", "slot", "card")]);
  assert.deepEqual(parent.childNodes.map((n) => n.getAttribute("data-key")), ["a", "c", "slot"]);
  assert.equal(parent.childNodes[2], slot);
  // Unkeyed nodes still match by place.
  const t = new Field("div", {}, ["x"]);
  const p2 = new Field("div", {}, []);
  p2.childNodes = [t];
  patch(p2, [new Field("div", {}, ["y"])]);
  assert.equal(p2.childNodes[0], t);
});

