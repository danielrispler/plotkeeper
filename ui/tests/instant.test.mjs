// Instant feedback (danielrispler/Falinks#570): a pressed row shows selected before the shell replies, and an update keeps
// the nodes that stay, so focus and text selection survive it. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El } from "./dom.mjs";
import { patch } from "../../dist/dom.js";
import { isSelected } from "../../dist/selection.js";
import { studio } from "../../dist/state.js";

// A selection as the core sends it, less the session's fields the mark never reads.
/** @returns {import("../../dist/types.js").Selection} */
const selection = (id, agent = null) => /** @type {import("../../dist/types.js").Selection} */ ({ session: { id }, agent });

test("a pressed row shows selected before the shell replies", () => {
  studio.selection = selection("a");
  studio.pending = { id: "b", agent: null };
  assert.equal(isSelected("b", null), true);
  assert.equal(isSelected("a", null), false);
  // The strip marks the session whichever of its lines is pressed.
  assert.equal(isSelected("b"), true);
});

test("a reply that disagrees puts the mark back", () => {
  studio.pending = null;
  studio.selection = selection("a");
  assert.equal(isSelected("a", null), true);
  assert.equal(isSelected("b", null), false);
});

test("a subagent's line is its own row", () => {
  studio.pending = null;
  studio.selection = selection("a", "w1");
  assert.equal(isSelected("a", "w1"), true);
  assert.equal(isSelected("a", null), false);
  assert.equal(isSelected("a"), true);
});

const row = (id, label, pressed) =>
  new El("button", { "data-key": id, "aria-pressed": String(pressed) }, [new El("span", {}, [label])]);

test("an update keeps the focused row and the selected text, and changes them in place", () => {
  const list = new El("div", {}, [row("a", "Alpha", true), row("b", "Beta", false)]);
  const focused = list.childNodes[1];
  const selectedText = focused.childNodes[0].childNodes[0];
  patch(list, [row("a", "Alpha", false), row("b", "Beta, now running", true), row("c", "Gamma", false)]);
  assert.equal(list.childNodes[1], focused);
  assert.equal(focused.childNodes[0].childNodes[0], selectedText);
  assert.equal(selectedText.nodeValue, "Beta, now running");
  assert.equal(focused.getAttribute("aria-pressed"), "true");
  assert.equal(list.childNodes[0].getAttribute("aria-pressed"), "false");
  assert.equal(list.childNodes.length, 3);
});

test("a different session in a row's place is a new row, and a gone one is dropped", () => {
  const list = new El("div", {}, [row("a", "Alpha", false), row("b", "Beta", false)]);
  const [a] = list.childNodes;
  patch(list, [row("b", "Beta", false)]);
  assert.notEqual(list.childNodes[0], a);
  assert.equal(list.childNodes[0].getAttribute("data-key"), "b");
  assert.equal(list.childNodes.length, 1);
});
