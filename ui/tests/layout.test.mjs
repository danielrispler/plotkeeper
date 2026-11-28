// The page's frame (spec #380, ticket #386): Treatment C's top bar, the map and the session list at the right, run on the `tsc` output.
// Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { arrange } from "../../dist/layout.js";
import { askedProject, startProject, wasRemoved } from "../../dist/map-model.js";

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

test("the page opens on the project its address asks for, else the last viewed project's map, else the first project's", () => {
  const saved = ["/w/falinks", "/w/plotkeeper", "/w/pokedex"];
  assert.equal(startProject([null, "/w/plotkeeper"], saved, saved), "/w/plotkeeper");
  // A start in a repository asks for its map (#756), over the last viewed; one not saved, a temporary one, gives way to it.
  assert.equal(startProject(["/w/pokedex", "/w/plotkeeper"], saved, saved), "/w/pokedex");
  assert.equal(startProject(["/tmp/scratch", "/w/plotkeeper"], saved, saved), "/w/plotkeeper");
  // Never viewed, or since removed: the first project.
  assert.equal(startProject([null, null], saved, saved), "/w/falinks");
  assert.equal(startProject([null, "/w/removed"], saved, saved), "/w/falinks");
  // A fresh install has none.
  assert.equal(startProject([null, "/w/plotkeeper"], [], []), null);
});

test("the address names the project to open as its fragment's project, spelled as the server encodes it", () => {
  assert.equal(askedProject("#project=/Users/d/my%20repo%2Bplus"), "/Users/d/my repo+plus");
  assert.equal(askedProject(""), null);
  assert.equal(askedProject("#other=1"), null);
});

test("the page falls back to the first project the list shows, while a last viewed project the list hides still opens", () => {
  const saved = ["/tmp/scratch", "/w/gone", "/w/plotkeeper"];
  const listed = ["/w/plotkeeper"];
  assert.equal(startProject([null, null], saved, listed), "/w/plotkeeper");
  assert.equal(startProject([null, "/w/removed"], saved, listed), "/w/plotkeeper");
  // Its "gone, Remove project" view is unchanged.
  assert.equal(startProject([null, "/w/gone"], saved, listed), "/w/gone");
  // With every saved project hidden (or the sessions not yet read), the first saved one, rather than the fresh install's words.
  assert.equal(startProject([null, null], saved, []), "/tmp/scratch");
});

test("the shown project gives way only when it was saved and is removed, not when it was never saved", () => {
  assert.equal(wasRemoved("/w/a", ["/w/a", "/w/b"], ["/w/b"]), true);
  assert.equal(wasRemoved("/w/a", ["/w/a"], ["/w/a", "/w/b"]), false);
  // A temporary project, never saved, or a live session's unsaved one, stays shown through another project's change.
  assert.equal(wasRemoved("/tmp/t", ["/w/a"], ["/w/a", "/w/b"]), false);
  assert.equal(wasRemoved("/tmp/t", [], ["/w/a"]), false);
});
