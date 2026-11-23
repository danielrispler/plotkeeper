// Home's waiting room, after the outside review of #253. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { Field, fakeDocument, tauri } from "./dom.mjs";

const calls = [];
tauri(calls);
const { drawHome, resetHome, homeState } = await import("../../dist/home.js");
globalThis.document = fakeDocument({ visibilityState: "visible", hasFocus: () => true }, Field);

const live = (id, state, extra = {}) => ({ id, state, unseen: false, landed: false, turns: [], questions: [], transcript: {}, identity: { issue: null, name: "" }, folder_name: "p", cwd: "/w/p", capabilities: { agent: "claude" }, name: id, ...extra });
const settle = () => new Promise((r) => setTimeout(r, 30));

test("a running row's elapsed time and an unseen row's age count up like the list's (C12)", () => {
  resetHome();
  const box = new Field("div");
  const now = Date.now();
  drawHome(box, [live("r", "running", { turn_span: { start: now - 60_000 } }), live("u", "your_turn", { unseen: true, turns: [{ ended_at: now - 60_000 }] })], []);
  const clocks = box.find("span").filter((n) => n.className === "clock");
  assert.equal(clocks.length, 2);
  for (const c of clocks) assert.match(String(c.dataset.key), /^count /);
});

test("an unseen row has two lines and the status shape beside the name (S6)", () => {
  resetHome();
  const box = new Field("div");
  drawHome(box, [live("u", "your_turn", { unseen: true, turns: [{ ended_at: Date.now() }] })], []);
  const row = box.find("button").find((b) => b.dataset.key === "unseen u");
  assert.ok(row.find("span").some((n) => n.dataset.glyph === "filled"));
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

