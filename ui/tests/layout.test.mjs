// The UI's pure functions, run on the `tsc` output (#465). Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { arrange, defaultWidths, paneBoxes, panelHeight, railShown, resized } from "../../dist/layout.js";
import { stripLabel } from "../../dist/list.js";

// The cell of 12.5px SF Mono in WebKit, as #577 measured it, and the terminal's padding, which holds no column.
const cell = 7.73;
const padding = 28;
const columns = (window, { list, rail }) => Math.floor((window - list - rail - padding) / cell);
const shape = (window, split = false) => arrange(window, cell, defaultWidths, { split });

test("the list and the rail start at 260 and 380 pixels, the strip at 56", () => {
  assert.deepEqual(defaultWidths, { list: 260, rail: 380 });
  assert.equal(shape(1100).list, 56);
});

test("at 2,560 and 1,920 pixels, everything shows in full", () => {
  for (const window of [2560, 1920]) assert.deepEqual(shape(window), { mode: "full", list: 260, rail: 380, stacked: false });
});

test("on the laptop's 1,512 pixels, the rail stays on screen beside a terminal of 100 columns", () => {
  const laptop = shape(1512);
  assert.deepEqual(laptop, { mode: "full", list: 260, rail: 380, stacked: false });
  assert.ok(columns(1512, laptop) >= 100);
});

test("the list gives way first, then the rail narrows toward 320 pixels", () => {
  const at1200 = shape(1200);
  assert.deepEqual(at1200, { mode: "strip", list: 56, rail: 343, stacked: false });
  assert.equal(columns(1200, at1200), 100);
});

test("then the terminal goes from 100 to 80 columns, the rail at 320", () => {
  const at1100 = shape(1100);
  assert.deepEqual(at1100, { mode: "strip", list: 56, rail: 320, stacked: false });
  assert.equal(columns(1100, at1100), 90);
  assert.equal(columns(1030, shape(1030)), 80);
});

test("last, the rail becomes an overlay", () => {
  assert.deepEqual(shape(1000), { mode: "overlay", list: 56, rail: 0, stacked: false });
});

test("a split sits side by side only while each pane gets 100 columns", () => {
  assert.deepEqual(shape(2560, true), { mode: "full", list: 260, rail: 380, stacked: false });
  // The list gives way before the split stacks.
  assert.deepEqual(shape(2200, true), { mode: "strip", list: 56, rail: 380, stacked: false });
  // Stacked, each pane has the column's whole width, so the list and the rail come back.
  for (const window of [1920, 1512]) assert.deepEqual(shape(window, true), { mode: "full", list: 260, rail: 380, stacked: true });
  assert.deepEqual(shape(1200, true), { mode: "strip", list: 56, rail: 343, stacked: true });
});

test("focus mode folds the list and the rail, and the same key restores them", () => {
  for (const window of [1200, 1512, 1920, 2560]) {
    assert.deepEqual(arrange(window, cell, defaultWidths, { focus: true }), { mode: "focus", list: 0, rail: 0, stacked: false });
    assert.deepEqual(arrange(window, cell, defaultWidths, { focus: false }), arrange(window, cell));
  }
  // A split in focus mode stacks only when the whole window cannot give each pane 100 columns.
  assert.equal(arrange(1512, cell, defaultWidths, { focus: true, split: true }).stacked, true);
  assert.equal(arrange(1920, cell, defaultWidths, { focus: true, split: true }).stacked, false);
});

test("a dragged divider stops at its column's least width", () => {
  assert.equal(resized("list", 50, defaultWidths, "full", 1920, cell).list, 160);
});

test("a dragged divider stops where the terminal would lose its 100 columns", () => {
  const { rail } = resized("rail", 5000, defaultWidths, "full", 1920, cell);
  assert.equal(arrange(1920, cell, { ...defaultWidths, rail }).mode, "full");
  assert.equal(arrange(1920, cell, { ...defaultWidths, rail: rail + 10 }).mode, "strip");
});

// The terminal's body, the window less the list and the rail, at a height of 900 px; a pane's columns
// from its box of insets.
const body = (window, split) => window - Object.values(arrange(window, cell, defaultWidths, { split })).filter(Number.isFinite).reduce((a, b) => a + b, 0);
const paneColumns = (width, { left, right }) => Math.floor((width - left - right) / cell);

test("one pane fills the terminal's body, inside its margin", () => {
  assert.deepEqual(paneBoxes(1100, 900, cell, null), [{ top: 6, right: 6, bottom: 6, left: 6 }]);
});

test("a split side by side halves the body, each pane at 100 columns or more", () => {
  const width = body(2560, true);
  assert.equal(arrange(2560, cell, defaultWidths, { split: true }).stacked, false);
  const [first, second] = paneBoxes(width, 900, cell, { stacked: false, share: 0.5 });
  assert.deepEqual(first, { top: 6, right: 966, bottom: 6, left: 6 });
  assert.deepEqual(second, { top: 6, right: 6, bottom: 6, left: 966 });
  for (const box of [first, second]) assert.ok(paneColumns(width, box) >= 100);
});

test("a stacked split halves the body's height, each pane its whole width", () => {
  const width = body(1512, true);
  assert.equal(arrange(1512, cell, defaultWidths, { split: true }).stacked, true);
  assert.deepEqual(paneBoxes(width, 900, cell, { stacked: true, share: 0.5 }), [
    { top: 6, right: 6, bottom: 456, left: 6 },
    { top: 456, right: 6, bottom: 6, left: 6 },
  ]);
});

test("a dragged split divider moves the share, and stops where a pane would get too small", () => {
  assert.deepEqual(paneBoxes(1920, 900, cell, { stacked: false, share: 0.6 })[1].left, 1158);
  // Side by side, each pane keeps 80 columns; stacked, 120 pixels.
  const [first] = paneBoxes(1920, 900, cell, { stacked: false, share: 0.05 });
  assert.equal(paneColumns(1920, first), 80);
  assert.equal(paneBoxes(1920, 900, cell, { stacked: true, share: 0.99 })[1].top, 786);
});

// The terminal panel (#542, #565 decision 5): a band under the terminal column, costing height only.
test("the panel takes its dragged height, at most 40 % of the terminal column and at least 80 px", () => {
  assert.equal(panelHeight(240, 900), 240);
  assert.equal(panelHeight(700, 900), 360);
  assert.equal(panelHeight(20, 900), 80);
  // A column too short for both keeps the 40 %.
  assert.equal(panelHeight(20, 150), 60);
});

test("the strip names a session by its issue number, else by its folder", () => {
  assert.equal(stripLabel("issue-338-205-pilots-final-freeze"), "338");
  assert.equal(stripLabel("Falinks"), "Fali");
});

test("a turn is seen only while the rail shows its session: not in focus mode, a closed overlay or the skills browser (#556)", () => {
  assert.equal(railShown("full", false, false), true);
  assert.equal(railShown("strip", false, false), true);
  assert.equal(railShown("focus", false, false), false);
  assert.equal(railShown("overlay", false, false), false);
  assert.equal(railShown("overlay", true, false), true);
  assert.equal(railShown("full", true, true), false);
});
