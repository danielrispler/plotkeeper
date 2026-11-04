// The UI's pure functions, run on the `tsc` output (#465). Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { layoutMode } from "../../dist/layout.js";
import { stripLabel } from "../../dist/list.js";

// The cell of 12.5px SF Mono in headless Chrome, and the three windows the #463 pick measured.
const cell = 7.526;

test("the full list shows while the terminal keeps 100 columns", () => {
  assert.equal(layoutMode(1920, cell), "full");
});

test("the list collapses to its strip first", () => {
  assert.equal(layoutMode(1512, cell), "strip");
});

test("below that, the rail becomes an overlay", () => {
  assert.equal(layoutMode(1280, cell), "overlay");
});

test("the strip names a session by its issue number, else by its folder", () => {
  assert.equal(stripLabel("issue-338-205-pilots-final-freeze"), "338");
  assert.equal(stripLabel("Falinks"), "Fali");
});
