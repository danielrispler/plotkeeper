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

test("the page opens on the last viewed project's map, else the first project's", () => {
  const saved = ["/w/falinks", "/w/plotkeeper", "/w/pokedex"];
  assert.equal(startProject("/w/plotkeeper", saved), "/w/plotkeeper");
  // Never viewed, or since removed: the first project.
  assert.equal(startProject(null, saved), "/w/falinks");
  assert.equal(startProject("/w/removed", saved), "/w/falinks");
  // A fresh install has none.
  assert.equal(startProject("/w/plotkeeper", []), null);
});
