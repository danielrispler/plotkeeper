// The listener path of `hint()` (spec #582, ticket 2): a summary, an input or a select, which `interestfor` cannot reach, opens its
// hint on pointer and on focus after the shared delay, at once beside an open one, and closes it on leaving. Run: `npm test`.
import { test, mock, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument } from "./dom.mjs";
import { hint, hintOf } from "../../dist/dom.js";

const page = new Map();
globalThis.document = fakeDocument({
  getElementById: (id) => page.get(id) ?? page.set(id, document.createElement("div")).get(id),
});
const make = (tag, key, attrs = {}) => {
  const node = document.createElement(tag);
  node.setAttribute("data-key", key);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, v);
  return node;
};
beforeEach(() => mock.timers.enable({ apis: ["setTimeout"] }));
afterEach(() => mock.timers.reset());

test("a summary, an input and a select get a hint of their own, no interestfor and no title", () => {
  for (const tag of ["summary", "input", "select"]) {
    const node = hint(make(tag, `f-${tag}`, { title: "old" }), `About the ${tag}`);
    assert.equal(hintOf(node)?.textContent, `About the ${tag}`);
    assert.equal(hintOf(node)?.getAttribute("popover"), "hint");
    assert.equal(node.getAttribute("title"), null);
    assert.equal(node.getAttribute("interestfor"), null);
    assert.equal(node.getAttribute("aria-describedby"), hintOf(node)?.getAttribute("id"));
    assert.equal(hintOf(node)?.getAttribute("role"), "tooltip");
  }
});

test("a hint that only repeats the summary's text adds no description", () => {
  const node = hint(Object.assign(make("summary", "f-same"), {}), "x");
  node.textContent = "Brief";
  hint(node, "Brief");
  assert.equal(node.getAttribute("aria-describedby"), null);
});

test("pointer on a summary opens its hint, from the summary, after the delay; leaving closes it", () => {
  const node = hint(make("summary", "f-hover"), "Show or hide");
  const shown = hintOf(node);
  node.listeners.pointerenter({ pointerType: "mouse" });
  mock.timers.tick(499);
  assert.equal(shown.popoverOpen, false);
  mock.timers.tick(2);
  assert.equal(shown.popoverOpen, true);
  assert.equal(shown.source, node);
  node.listeners.pointerleave({});
  mock.timers.tick(200);
  assert.equal(shown.popoverOpen, false);
});

test("focus on an input opens its hint, and blur closes it", () => {
  const node = hint(make("input", "f-focus"), "The key");
  node.matches = () => true;
  node.listeners.focus({});
  mock.timers.tick(500);
  assert.equal(hintOf(node).popoverOpen, true);
  node.listeners.blur({});
  mock.timers.tick(200);
  assert.equal(hintOf(node).popoverOpen, false);
});

test("the pointer on the hint keeps it open", () => {
  const node = hint(make("select", "f-rest"), "Which model");
  const shown = hintOf(node);
  node.listeners.pointerenter({});
  mock.timers.tick(500);
  node.listeners.pointerleave({});
  shown.listeners.pointerenter({});
  mock.timers.tick(500);
  assert.equal(shown.popoverOpen, true);
  shown.listeners.pointerleave({});
  mock.timers.tick(200);
  assert.equal(shown.popoverOpen, false);
});

test("a neighbour's hint opens at once while one is open", () => {
  const bar = document.createElement("div");
  const [a, b] = [hint(make("summary", "f-a"), "A words"), hint(make("summary", "f-b"), "B words")];
  bar.append(a, b);
  a.parentElement = b.parentElement = bar;
  a.listeners.pointerenter({});
  mock.timers.tick(500);
  assert.equal(hintOf(a).popoverOpen, true);
  a.listeners.pointerleave({});
  b.listeners.pointerenter({});
  mock.timers.tick(1);
  assert.equal(hintOf(b).popoverOpen, true);
});

test("a hint drawn again for the same trigger keeps one hint and one set of listeners", () => {
  const node = make("input", "f-again");
  hint(node, "one");
  const listener = node.listeners.pointerenter;
  hint(node, "two");
  assert.equal(hintOf(node).textContent, "two");
  assert.equal(node.listeners.pointerenter, listener);
});
