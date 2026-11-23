// The end-of-story bar's logic (spec danielrispler/plotkeeper#378, ticket #393), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { barKey, barModel, explainsBack, LINE_MAX, MARKS, NOTE_MAX } from "../../dist/story-bar.js";

const recorder = () => {
  const sent = [];
  return { sent, send: (kind, extra = {}) => sent.push({ kind, ...extra }) };
};
const key = (k, extra = {}) => ({ key: k, metaKey: false, ctrlKey: false, altKey: false, ...extra });

test("the bar is offered once, and a rating of 1 to 5 is logged each time one is picked", () => {
  const { sent, send } = recorder();
  const bar = barModel(send);
  bar.offer();
  bar.offer();
  bar.rate(4);
  bar.rate(0);
  bar.rate(6);
  bar.rate(2);
  assert.deepEqual(sent, [{ kind: "rating-offered" }, { kind: "rating-given", rating: 4 }, { kind: "rating-given", rating: 2 }]);
  assert.equal(bar.rated(), 2, "the pick stays pressed while the bar is open");
  assert.equal(barModel(send).rated(), null, "a bar opened again starts empty: no rating is shown back");
});

test("a note is logged trimmed, and an empty or too long one is not", () => {
  const { sent, send } = recorder();
  const bar = barModel(send);
  assert.equal(bar.note("   "), false);
  assert.equal(bar.note("x".repeat(NOTE_MAX + 1)), false);
  assert.equal(bar.note("  The second step lost me. "), true);
  assert.deepEqual(sent, [{ kind: "note-given", note: "The second step lost me." }]);
});

test("skipping closes the bar; it is logged as a skip only when nothing was given", () => {
  const { sent, send } = recorder();
  const bar = barModel(send);
  bar.skip();
  bar.skip();
  bar.rate(3);
  assert.deepEqual(sent, [{ kind: "rating-skipped" }], "a closed bar takes nothing more");
  assert.equal(bar.closed(), true);
  const rated = recorder();
  const after = barModel(rated.send);
  after.rate(5);
  after.skip();
  assert.deepEqual(rated.sent, [{ kind: "rating-given", rating: 5 }], "a skip after a rating only closes the bar");
  assert.equal(after.closed(), true);
});

test("one key skips the bar and the digits rate, never while typing in the note or with a modifier", () => {
  assert.equal(barKey(key("s"), false), "skip");
  assert.deepEqual(barKey(key("3"), false), { rate: 3 });
  assert.equal(barKey(key("6"), false), null);
  assert.equal(barKey(key("0"), false), null);
  assert.equal(barKey(key("x"), false), null);
  assert.equal(barKey(key("s"), true), null, "typing an s in the note");
  assert.equal(barKey(key("4"), true), null, "typing a 4 in the note");
  assert.equal(barKey(key("s", { metaKey: true }), false), null);
  assert.equal(barKey(key("2", { ctrlKey: true }), false), null);
});

// ---- explain-back (#397) ----

test("explain-back is offered only on a story of a Core part that has a gist; the rating is on every story", () => {
  const tierOf = (part) => ({ shell: "core", face: "interface", box: "black-box" })[part] ?? "core";
  assert.equal(explainsBack("The shell keeps fog.", ["face", "shell"], tierOf), true);
  assert.equal(explainsBack("The face shows it.", ["face", "box"], tierOf), false, "no Core part");
  assert.equal(explainsBack("", ["shell"], tierOf), false, "a story written before the gist, or of no Core part");
  assert.equal(explainsBack(undefined, ["shell"], tierOf), false);
});

test("the explain-back line is logged trimmed, then each mark; no number comes back", () => {
  const { sent, send } = recorder();
  const bar = barModel(send);
  assert.equal(bar.mark("got-it"), false, "nothing to mark before a line");
  assert.equal(bar.explain("   "), false);
  assert.equal(bar.explain("x".repeat(LINE_MAX + 1)), false);
  assert.equal(bar.explain("  The shell keeps fog until I read it. "), true);
  assert.equal(bar.explained(), "The shell keeps fog until I read it.");
  assert.equal(bar.explain("again"), false, "one line per bar");
  assert.equal(bar.mark("partly"), true);
  assert.equal(bar.mark("so-so"), false);
  assert.equal(bar.mark("missed"), true);
  assert.equal(bar.mark("got-it"), true);
  assert.equal(bar.marked(), "got-it");
  assert.deepEqual(sent, [
    { kind: "explain-back-given", line: "The shell keeps fog until I read it." },
    { kind: "explain-back-marked", mark: "partly" },
    { kind: "explain-back-marked", mark: "missed" },
    { kind: "explain-back-marked", mark: "got-it" },
  ]);
  assert.deepEqual(
    MARKS.map((m) => m.label),
    ["Got it", "Partly", "Missed"],
  );
  assert.ok(MARKS.every((m) => !/\d/.test(m.label)), "a mark is words, never a number");
});

test("a skip after an explain-back line only closes the bar, and the keys stay off while the line is typed", () => {
  const { sent, send } = recorder();
  const bar = barModel(send);
  bar.explain("It keeps fog.");
  bar.skip();
  assert.deepEqual(sent.map((s) => s.kind), ["explain-back-given"]);
  assert.equal(bar.mark("got-it"), false, "a closed bar takes nothing more");
  // The line is an input: the window passes typing, so neither S nor a digit acts on the bar.
  assert.equal(barKey(key("s"), true), null);
  assert.equal(barKey(key("1"), true), null);
});
