// The page's frame (spec #380, ticket #386): Treatment C's top bar, the map and the session list at the right, run on the `tsc` output.
// Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { arrange } from "../../dist/layout.js";
import { startProject } from "../../dist/map-model.js";

test("from 1,007 pixels up the list shows in full, 252 pixels wide, beside the map", () => {
  for (const window of [2560, 1920, 1512, 1007]) assert.deepEqual(arrange(window), { mode: "full", list: 252 });
});

test("below 1,007 pixels the list is its strip, as before the map became the base", () => {
  for (const window of [1006, 1000, 864]) assert.deepEqual(arrange(window), { mode: "strip", list: 56 });
});

test("focus mode folds the list, and the same key restores it", () => {
  for (const window of [1000, 1512, 1920]) {
    assert.deepEqual(arrange(window, { focus: true }), { mode: "focus", list: 0 });
    assert.deepEqual(arrange(window, { focus: false }), arrange(window));
  }
});

// Toggle list in a wide window (spec #658, user stories 29–31): the strip in the full list's place, and the same key restores it.
test("a folded list in a wide window is its strip, and unfolded it is the full list again", () => {
  for (const window of [2560, 1512, 1007]) {
    assert.deepEqual(arrange(window, { folded: true }), { mode: "strip", list: 56 });
    assert.deepEqual(arrange(window, { folded: false }), { mode: "full", list: 252 });
  }
  // Narrow, the list is its strip either way; focus mode still folds it whole.
  assert.deepEqual(arrange(1000, { folded: true }), { mode: "strip", list: 56 });
  assert.deepEqual(arrange(1512, { folded: true, focus: true }), { mode: "focus", list: 0 });
});

test("the page opens on the last viewed project's map, else the first project's", () => {
  const saved = ["/w/falinks", "/w/plotkeeper", "/w/pokedex"];
  assert.equal(startProject("/w/plotkeeper", saved), "/w/plotkeeper");
  // Never viewed, or since removed: the first project.
  assert.equal(startProject(null, saved), "/w/falinks");
  assert.equal(startProject("/w/removed", saved), "/w/falinks");
  // A fresh install has none.
  assert.equal(startProject("/w/plotkeeper", []), null);
});
