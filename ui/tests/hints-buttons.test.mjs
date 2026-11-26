// The hints of the page's other buttons and links (spec #582, ticket 3): each enabled one has its own hint of the words its title had,
// and no `title`. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { El, fakeDocument } from "./dom.mjs";

// Loaded before the page's document exists, so `times.ts` starts no clock that keeps the run alive.
const { hintOf } = await import("../../dist/dom.js");
const { toastButtons } = await import("../../dist/toast.js");
const { runsNodes } = await import("../../dist/map-runs.js");
const { preFlightNodes } = await import("../../dist/map-pre-flight.js");
const { briefNodes } = await import("../../dist/cards.js");
globalThis.document = fakeDocument({ body: new El("body") });

const words = (button) => hintOf(button)?.textContent;
// The page's nodes, which here are the stand-in DOM's, whose fields the tests read.
/** @type {(nodes: HTMLElement[]) => any[]} */
const stand = (nodes) => nodes;

test("a toast's inner buttons have hints of their own, and its container has no title", () => {
  const toast = new El("div");
  toast.id = "probe-toast";
  const { main, close } = /** @type {any} */ (toastButtons)(toast, () => new El("button"), () => {}, "Hide this toast; the clear goes on");
  assert.equal(words(close), "Hide this toast; the clear goes on");
  assert.equal(close.getAttribute("title"), null);
  assert.equal(toast.getAttribute("title"), null);
  assert.equal(main.tag, "button");
});

test("a run's Cancel button has a hint and no title", () => {
  /** @type {any} */
  const queued = { id: "a", kind: "back-law", state: "queued", at: 0, outcome: null, line: null, denials: [], prompt: null };
  const row = stand(runsNodes({ runs: [queued], paused: null }, { cancel() {}, confirm: async () => true, resume() {}, copy: async () => "copied" })).find((node) => node.find("button").length);
  const cancel = row.find("button").find((each) => each.text === "Cancel");
  assert.equal(words(cancel), "Take this run out of the queue");
  assert.equal(cancel.getAttribute("title"), null);
  assert.equal(cancel.title, undefined);
});

test("pre-flight's buttons have hints and no title", () => {
  const act = { predict() {}, byHand() {}, pick() {}, clear() {} };
  const nodes = preFlightNodes({ kind: "start", issues: [{ number: 394, title: "Studio: pre-flight" }] }, { ok: true, parts: [], actors: [], links: [], laws: [] }, /** @type {any} */ ({}), act);
  const buttons = stand(nodes).flatMap((node) => node.find("button"));
  assert.ok(buttons.length > 0);
  for (const button of buttons) {
    assert.ok(words(button), `no hint on ${button.text}`);
    assert.equal(button.getAttribute("title"), null);
  }
});

test("the page's HTML gives no button or link a title", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const tags = html.match(/<(button|a)\b[^>]*>/g) ?? [];
  assert.deepEqual(tags.filter((tag) => /\stitle=/.test(tag)), []);
});

test("a design line's citation is named by its hint's words, read once: no description repeats its name", () => {
  const cards = { brief: null, goal: null, answers: [], open: [], design: [{ text: "Use one hint", cite: "t1" }], decisions: [], risk_notes: [] };
  const line = stand(/** @type {any} */ (briefNodes)(cards, () => {})).flatMap((node) => node.childNodes ?? []);
  const cite = line.find((node) => node?.tag === "button");
  assert.equal(cite.getAttribute("aria-label"), "Open the turn it came from");
  assert.equal(words(cite), "Open the turn it came from");
  assert.equal(cite.getAttribute("aria-describedby"), null);
});
