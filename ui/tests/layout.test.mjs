// The UI's pure functions, run on the `tsc` output (danielrispler/Falinks#465). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { homeSeen, arrange, defaultWidths, railShown, resized } from "../../dist/layout.js";

// The cell of 12.5px SF Mono in WebKit, as danielrispler/Falinks#577 measured it, and the terminal's padding, which holds no column.
const cell = 7.73;
const padding = 28;
const columns = (window, { list, rail }) => Math.floor((window - list - rail - padding) / cell);
const shape = (window) => arrange(window, cell, defaultWidths);

test("at 2,560 and 1,920 pixels, everything shows in full", () => {
  for (const window of [2560, 1920]) assert.deepEqual(shape(window), { mode: "full", list: 260, rail: 380 });
});

test("on the laptop's 1,512 pixels, the rail stays on screen beside a terminal of 100 columns", () => {
  const laptop = shape(1512);
  assert.deepEqual(laptop, { mode: "full", list: 260, rail: 380 });
  assert.ok(columns(1512, laptop) >= 100);
});

test("the list gives way first, then the rail narrows toward 320 pixels", () => {
  const at1200 = shape(1200);
  assert.deepEqual(at1200, { mode: "strip", list: 56, rail: 343 });
  assert.equal(columns(1200, at1200), 100);
});

test("then the terminal goes from 100 to 80 columns, the rail at 320", () => {
  const at1100 = shape(1100);
  assert.deepEqual(at1100, { mode: "strip", list: 56, rail: 320 });
  assert.equal(columns(1100, at1100), 90);
  assert.equal(columns(1030, shape(1030)), 80);
});

test("last, the rail becomes an overlay", () => {
  assert.deepEqual(shape(1000), { mode: "overlay", list: 56, rail: 0 });
});

test("focus mode folds the list and the rail, and the same key restores them", () => {
  for (const window of [1200, 1512, 1920, 2560]) {
    assert.deepEqual(arrange(window, cell, defaultWidths, { focus: true }), { mode: "focus", list: 0, rail: 0 });
    assert.deepEqual(arrange(window, cell, defaultWidths, { focus: false }), arrange(window, cell));
  }
});

test("a dragged divider stops at its column's least width", () => {
  assert.equal(resized("list", 50, defaultWidths, "full", 1920, cell).list, 160);
});

test("a dragged divider stops where the terminal would lose its 100 columns", () => {
  const { rail } = resized("rail", 5000, defaultWidths, "full", 1920, cell);
  assert.equal(arrange(1920, cell, { ...defaultWidths, rail }).mode, "full");
  assert.equal(arrange(1920, cell, { ...defaultWidths, rail: rail + 10 }).mode, "strip");
});

test("a turn is seen only while the rail shows its session: not in focus mode or a closed overlay (danielrispler/Falinks#556)", () => {
  assert.equal(railShown("full", false, false), true);
  assert.equal(railShown("strip", false, false), true);
  assert.equal(railShown("focus", false, false), false);
  assert.equal(railShown("overlay", false, false), false);
  assert.equal(railShown("overlay", true, false), true);
  // The Map view hides the rail without taking it out of the viewport, so a turn behind it is not seen.
  assert.equal(railShown("full", false, true), false);
  assert.equal(railShown("overlay", true, true), false);
});

test("Home's marks and learning card are seen only while Home is open and the Map view is not over it", () => {
  assert.equal(homeSeen(true, false), true);
  assert.equal(homeSeen(false, false), false);
  assert.equal(homeSeen(true, true), false);
});
