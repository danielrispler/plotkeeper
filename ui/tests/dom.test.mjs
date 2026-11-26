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


// Chrome fires a dialog's close event at the next frame, so a close from an earlier ask can land after the dialog was opened again: in
// snap's loaded runs (#459) Cancel's late close answered the next ask before its confirm was pressed. A close while the dialog is open is
// an earlier ask's, and leaves this ask waiting.
test("an earlier ask's late close does not answer the confirm opened after it", async () => {
  const { confirmed } = await import("../../dist/dom.js");
  /** @type {any} */
  const Dialog = class Dialog extends Field {};
  globalThis.HTMLDialogElement = Dialog;
  const form = { onsubmit: null };
  const dialog = Object.assign(new Dialog("dialog"), { open: false, querySelector: () => form, showModal: () => (dialog.open = true) });
  const fields = new Map([["confirm-dialog", dialog]]);
  const was = document.getElementById;
  document.getElementById = /** @type {any} */ ((id) => (fields.has(id) ? fields.get(id) : fields.set(id, new Field("div")).get(id)));
  try {
    const ask = { heading: "Remove?", lost: "The key.", verb: "Remove" };
    const first = confirmed(ask);
    dialog.open = false;
    form.onsubmit({ submitter: { value: "cancel" } });
    assert.equal(await first, false);
    const second = confirmed(ask);
    dialog.onclose();
    dialog.open = false;
    form.onsubmit({ submitter: { value: "confirm" } });
    assert.equal(await second, true, "the first ask's close, landing late, answered the second");
  } finally {
    document.getElementById = was;
  }
});

// `element(id, type)` returns the element as that type (#554), and a page whose element is of another type fails with the id and both
// types, not later as a missing method.
test("element with a type returns that type and names a wrong one", async () => {
  const { element } = await import("../../dist/dom.js");
  /** @type {any} */
  const Dialog = class Dialog extends Field {};
  /** @type {any} */
  const Input = class Input extends Field {};
  const dialog = new Dialog("dialog");
  const was = document.getElementById;
  document.getElementById = (id) => (id === "confirm-dialog" ? dialog : null);
  try {
    assert.equal(element("confirm-dialog", Dialog), dialog);
    assert.throws(() => element("confirm-dialog", Input), { message: "#confirm-dialog is Dialog, not Input" });
    assert.throws(() => element("gone"), { message: "#gone is not on the page" });
  } finally {
    document.getElementById = was;
  }
});

// The hint (spec #582): a button's or link's own `popover="hint"`, wired by `interestfor`, with the ARIA rule and no `title`.
const buttonWith = (label, attrs = {}) => new Field("button", attrs, [label]);

test("a hint gives a button its own hint of the text, wired by interestfor, and no title", async () => {
  const { hint, hintOf } = await import("../../dist/dom.js");
  const button = buttonWith("Go", { title: "old words" });
  assert.equal(hint(button, "Go to the first session that needs you"), button);
  const target = hintOf(button);
  assert.equal(target?.textContent, "Go to the first session that needs you");
  assert.equal(target?.getAttribute("popover"), "hint");
  assert.equal(button.getAttribute("interestfor"), target?.getAttribute("id"));
  assert.equal(button.getAttribute("title"), null);
});

test("a hint that adds to the trigger's name is its description; one that repeats it adds nothing", async () => {
  const { hint, hintOf } = await import("../../dist/dom.js");
  const adds = hint(buttonWith("Go", { "data-key": "adds" }), "Go to the first session");
  assert.equal(hintOf(adds)?.getAttribute("role"), "tooltip");
  assert.equal(adds.getAttribute("aria-describedby"), adds.getAttribute("interestfor"));
  const repeats = hint(buttonWith("", { "data-key": "repeats", "aria-label": "Settings" }), "Settings");
  assert.equal(hintOf(repeats)?.getAttribute("role"), null);
  assert.equal(repeats.getAttribute("aria-describedby"), null);
  const labelled = hint(buttonWith("  Close \n", { "data-key": "close" }), "Close");
  assert.equal(labelled.getAttribute("aria-describedby"), null);
});

test("a hint leaves the author's own aria-describedby, and is drawn again for the same trigger without a second hint", async () => {
  const { hint, hintOf } = await import("../../dist/dom.js");
  const own = hint(buttonWith("Go", { "data-key": "own", "aria-describedby": "elsewhere" }), "Go on");
  assert.equal(own.getAttribute("aria-describedby"), "elsewhere");
  const first = hintOf(hint(buttonWith("Go", { "data-key": "again" }), "One"));
  const second = hintOf(hint(buttonWith("Go", { "data-key": "again" }), "Two"));
  assert.equal(second, first);
  assert.equal(second?.textContent, "Two");
});
