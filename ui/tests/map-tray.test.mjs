// The Map view's "Ready for review" tray (tickets #336 and #339), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { trayRows } from "../../dist/map-tray.js";

const pr = (number, title, brief = null) => ({ number, title, head: "aaa", brief });

test("a ready PR with no brief yet is a row that cannot be opened and says so", () => {
  const rows = trayRows([pr(7, "Fog on the map"), pr(9, "")]);
  assert.deepEqual(rows.map((r) => [r.number, r.title, r.openable]), [[7, "Fog on the map", false], [9, "PR #9", false]]);
  assert.match(rows[0].note, /#7 .*not written yet/);
});

test("a brief being written says so, and a stored one opens", () => {
  const writing = trayRows([pr(7, "Fog", { stale: false, writing: true, chapters: [] })])[0];
  assert.equal(writing.openable, false);
  assert.match(writing.note, /writing/);
  const ready = trayRows([pr(7, "Fog", { stale: false, writing: false, chapters: ["Fog"] })])[0];
  assert.equal(ready.openable, true);
  assert.equal(ready.chapter, 0);
  assert.equal(ready.title, "Fog");
});

test("a stale brief still opens and says it is rewritten when opened", () => {
  const row = trayRows([pr(7, "Fog", { stale: true, writing: false, chapters: ["Fog"] })])[0];
  assert.equal(row.openable, true);
  assert.match(row.note, /newer push/);
});

test("a chaptered brief has one row per chapter, in order", () => {
  const rows = trayRows([pr(7, "PR briefs", { stale: false, writing: false, chapters: ["Fog", "Broken laws", "Ratify"] })]);
  assert.deepEqual(rows.map((r) => [r.chapter, r.title]), [[0, "Fog"], [1, "Broken laws"], [2, "Ratify"]]);
  assert.match(rows[1].note, /#7 .*PR briefs.*2 of 3/);
});

test("with nothing ready there is no row", () => {
  assert.deepEqual(trayRows([]), []);
});
