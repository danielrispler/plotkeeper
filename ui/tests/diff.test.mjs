// The diff pane (danielrispler/plotkeeper#131). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument, tauri } from "./dom.mjs";

globalThis.document = fakeDocument();
const { diffLines, drawDiff, newestDiff } = await import("../../dist/diff.js");

// A file without its `agent`, which the pane does not read.
/** @returns {import("../../dist/types.js").TurnFile} */
const file = (path, added, removed, extra = {}) => /** @type {any} */ ({ root: "/r", path, from: null, added, removed, ...extra });
// A turn with only its files and checks; the rest of a `Turn` is left out.
/** @returns {import("../../dist/types.js").Turn} */
const turn = (files) => /** @type {any} */ ({ files: files && { files }, checks: [] });

test("a hunk's lines are classified for the tint, with the sign kept in the text", () => {
  const lines = diffLines("diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1,2 @@\n ctx\n-old\n+new\n");
  assert.deepEqual(lines.map((l) => l.kind), ["meta", "meta", "meta", "hunk", "ctx", "del", "add"]);
  assert.equal(lines[5].text, "-old");
  assert.equal(lines[6].text, "+new");
});

test("the palette opens the newest turn with files; an agent without trees has none", () => {
  const turns = [turn([file("a", 1, 0)]), turn([file("b", 1, 0)]), turn([]), turn(null)];
  assert.equal(newestDiff(turns, { turn_trees: true }), 1);
  assert.equal(newestDiff(turns, { turn_trees: false }), null);
  assert.equal(newestDiff([turn(null)], { turn_trees: true }), null);
});

test("the pane lists the agent's files with counts, each hunk read whole, and no external fold", async () => {
  const calls = [];
  const hunks = { "a.rs": "@@ -1 +1,2 @@\n a\n+b\n", "img.png": null };
  tauri(calls, (_command, args) => (args.path === "img.png" ? { text: "", binary: true, first_line: 1 } : { text: hunks[args.path], binary: false, first_line: 2 }));
  const place = new El("div");
  let back = 0;
  drawDiff(place, { id: "s", turns: [turn([file("a.rs", 1, 0), file("img.png", null, null)])] }, 0, () => back++);
  await new Promise((resolve) => setTimeout(resolve));
  assert.deepEqual(calls.filter(([c]) => c === "turn_file_hunk").map(([c, a]) => [c, a.all, a.turn, a.path]), [["turn_file_hunk", true, 0, "a.rs"], ["turn_file_hunk", true, 0, "img.png"]]);
  // The self-test that was told of an opened hunk is retired (#348).
  assert.equal(calls.some(([c]) => c === "hunk_opened"), false);
  const summaries = place.find("summary").map((s) => s.text);
  assert.deepEqual(summaries, ["a.rs +1 −0", "img.png"]);
  assert.match(place.text, /\+b/);
  assert.match(place.text, /Binary file/);
  place.find("button").find((b) => b.text === "Back").listeners.click();
  assert.equal(back, 1);
});

test("a turn with no files says so, and draws no external line", () => {
  const place = new El("div");
  drawDiff(place, { id: "s", turns: [turn([])] }, 0, () => {});
  assert.match(place.text, /changed no files/);
  assert.equal(place.find("summary").length, 0);
});

test("the head counts the files and shows \"+N merged\" for the files a merge brought in", () => {
  const place = new El("div");
  const merged = { ...turn([file("a.rs", 1, 0)]), files: { files: [file("a.rs", 1, 0)], merged: 2 } };
  drawDiff(place, { id: "s", turns: [merged] }, 0, () => {});
  assert.match(place.text, /1 file · \+2 merged/);
  const plain = new El("div");
  drawDiff(plain, { id: "s", turns: [turn([file("a.rs", 1, 0)])] }, 0, () => {});
  assert.doesNotMatch(plain.text, /merged/);
});
