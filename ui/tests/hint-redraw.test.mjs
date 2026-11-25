// The listener path's hints across redraws (spec #582's review): a view drawn again through `patch` hands `hint()` a new trigger each
// time. The trigger the page keeps opens the hint its current `data-hint` names, with the current words; the hint gains no listeners per
// draw; and a hint no trigger on the page names any more, its trigger dropped, leaves `#hints`. Run: `npm test`.
import { test, mock, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import v8 from "node:v8";
import vm from "node:vm";
import { El, fakeDocument } from "./dom.mjs";

v8.setFlagsFromString("--expose_gc");
const gc = vm.runInNewContext("gc");

const page = new Map();
const body = new El("body");
globalThis.document = fakeDocument({
  getElementById: (id) => page.get(id) ?? page.set(id, document.createElement("div")).get(id),
  querySelectorAll: (selector) => body.querySelectorAll(selector),
});
const { hint, patch, keyed, hintOf: hintFor } = await import("../../dist/dom.js");
/** @type {(node: any) => any} */
const hintOf = (node) => hintFor(node);
/** @type {any} */
const hints = document.getElementById("hints");
const inHints = (id) => hints.childNodes.some((node) => node.getAttribute?.("id") === id);
const open = () => hints.childNodes.filter((node) => node.popoverOpen);

beforeEach(() => mock.timers.enable({ apis: ["setTimeout"] }));
// Every queued prune runs before the clock goes, so the next test's `hint()` queues its own.
afterEach(() => {
  mock.timers.tick(1000);
  for (const node of open()) node.hidePopover();
  mock.timers.reset();
});

/** A row of the page, drawn by `patch`. @returns {any} */
const row = () => {
  const node = new El("div");
  body.append(node);
  return node;
};

test("an unkeyed span drawn again opens the hint of its new words, not the first draw's", () => {
  const line = row();
  const draw = (/** @type {string} */ words) => [hint(document.createElement("span"), words)];
  patch(line, draw("turn words 1"));
  const [kept] = line.childNodes;
  patch(line, draw("turn words 2"));
  assert.equal(line.childNodes[0], kept, "patch keeps the span");
  kept.listeners.pointerenter({});
  mock.timers.tick(501);
  assert.deepEqual(open().map((node) => node.textContent), ["turn words 2"]);
  kept.listeners.pointerleave({});
  mock.timers.tick(200);
  assert.deepEqual(open(), []);
});

test("an unkeyed span drawn again leaves no hint of its first draw in #hints", () => {
  const line = row();
  const draw = (/** @type {string} */ words) => [hint(document.createElement("span"), words)];
  patch(line, draw("first words"));
  const first = line.childNodes[0].getAttribute("data-hint");
  patch(line, draw("second words"));
  mock.timers.tick(300);
  assert.equal(inHints(first), false);
  assert.ok(inHints(line.childNodes[0].getAttribute("data-hint")));
});

test("a keyed span drawn again ten times keeps one set of listeners on its hint", () => {
  const line = row();
  const draw = () => [hint(keyed(document.createElement("span"), "mark-redrawn"), "A plain terminal")];
  patch(line, draw());
  const shown = hintOf(line.childNodes[0]);
  const count = () => Object.values(shown.added).reduce((sum, all) => sum + all.length, 0);
  const once = count();
  for (let n = 0; n < 10; n++) patch(line, draw());
  assert.equal(count(), once);
});

test("a dropped span's hint leaves #hints once the span is gone", async () => {
  const line = row();
  const draw = () => [hint(keyed(document.createElement("span"), "mark-dropped"), "Dropped words")];
  patch(line, draw());
  patch(line, draw());
  const id = line.childNodes[0].getAttribute("data-hint");
  patch(line, []);
  // A WeakRef holds its target to the end of the job that read it: collect in a later one.
  await new Promise((done) => setImmediate(done));
  gc();
  hint(keyed(document.createElement("button"), "requeue-prune"), "Queues a prune");
  mock.timers.tick(300);
  assert.equal(inHints(id), false);
});
