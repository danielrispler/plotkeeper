// The end-of-story bar's logic (spec danielrispler/plotkeeper#378, ticket #393; trimmed by #448), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { barKey, barModel, explainsBack, LINE_MAX, MARKS } from "../../dist/story-bar.js";

const recorder = () => {
  const sent = [];
  return { sent, send: (kind, extra = {}) => sent.push({ kind, ...extra }) };
};
const key = (k, extra = {}) => ({ key: k, metaKey: false, ctrlKey: false, altKey: false, ...extra });

test("skipping closes the bar and logs nothing; a digit no longer rates (#448)", () => {
  const { sent, send } = recorder();
  const bar = barModel(send);
  bar.skip();
  assert.equal(bar.closed(), true);
  assert.equal(bar.explain("It keeps fog."), false, "a closed bar takes nothing more");
  assert.deepEqual(sent, []);
});

test("the S key skips, never while typing or with a modifier; no other key acts on the bar", () => {
  assert.equal(barKey(key("s"), false), "skip");
  assert.equal(barKey(key("S"), false), "skip");
  assert.equal(barKey(key("3"), false), null);
  assert.equal(barKey(key("x"), false), null);
  assert.equal(barKey(key("s"), true), null, "typing an s in the line");
  assert.equal(barKey(key("s", { metaKey: true }), false), null);
  assert.equal(barKey(key("s", { ctrlKey: true }), false), null);
});

// ---- explain-back (#397) ----

test("explain-back is offered only on a story of a Core part that has a gist; otherwise the bar holds only Skip", () => {
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
  // The line is an input: the window passes typing, so S does not act on the bar.
  assert.equal(barKey(key("s"), true), null);
});
