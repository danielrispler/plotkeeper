// The Map view's "Ready for review" tray (tickets #336 and #339), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { trayRows } from "../../dist/map-tray.js";

const pr = (number, title, brief = null) => ({ number, title, head: "aaa", brief });

test("a ready PR whose brief is not written is a row that opens, and opening it writes the brief", () => {
  const rows = trayRows([pr(7, "Fog on the map"), pr(9, "")]);
  assert.deepEqual(rows.map((r) => [r.number, r.title, r.openable, r.chapter]), [[7, "Fog on the map", true, 0], [9, "PR #9", true, 0]]);
  assert.match(rows[0].note, /#7 .*written when you open it/);
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

test("a ready PR that fogs nothing at the tiers says so and cannot be opened", () => {
  const row = trayRows([pr(7, "Docs", { stale: false, writing: false, chapters: [], quiet: true })])[0];
  assert.equal(row.openable, false);
  assert.match(row.note, /no brief/);
  assert.doesNotMatch(row.note, /not written yet/);
});

test("a PR whose write failed says so and opens to try again", () => {
  const none = trayRows([pr(7, "Fog", { stale: false, writing: false, chapters: [], quiet: false, failed: true })])[0];
  assert.equal(none.openable, true);
  assert.match(none.note, /#7 .*could not be written; open it to try again/);
  const stale = trayRows([pr(7, "Fog", { stale: true, writing: false, chapters: ["Fog"], quiet: false, failed: true })])[0];
  assert.equal(stale.openable, true);
  assert.match(stale.note, /could not be rewritten; open it to try again/);
});

test("without claude, a PR with no stored brief says briefs need Claude and does not open; a stored one still does", () => {
  const none = trayRows([pr(7, "Fog")], false)[0];
  assert.equal(none.openable, false);
  assert.match(none.note, /#7 · briefs need Claude/);
  const stored = trayRows([pr(8, "Fog", { stale: false, writing: false, chapters: ["Fog"] })], false)[0];
  assert.equal(stored.openable, true);
});

test("after a push the row says why the brief is what it is: the read carried over, or it was rewritten (#477, part B)", () => {
  const carried = trayRows([pr(7, "Fog", { stale: false, writing: false, chapters: ["Fog"], quiet: false, read: true, note: "Rebased onto main, nothing new in the PR" })])[0];
  assert.equal(carried.note, "#7 · Rebased onto main, nothing new in the PR");
  const chaptered = trayRows([pr(7, "PR briefs", { stale: false, writing: false, chapters: ["Fog", "Ratify"], quiet: false, note: "Rewritten after a new push; you had read 1 of 2" })]);
  assert.match(chaptered[0].note, /1 of 2 · Rewritten after a new push; you had read 1 of 2$/);
  assert.doesNotMatch(chaptered[1].note, /Rewritten/);
  // While the newer push is not yet rewritten, the row says that, not the note.
  const stale = trayRows([pr(7, "Fog", { stale: true, writing: false, chapters: ["Fog"], quiet: false, note: "Rewritten: the branch was rebased" })])[0];
  assert.match(stale.note, /newer push; rewritten when opened$/);
});

test("a head with a Since you read story opens it first, and the full brief's rows are one click away (#497)", () => {
  const rows = trayRows([pr(7, "PR briefs", { stale: false, writing: false, chapters: ["Fog", "Ratify"], quiet: false, since: "The fix since you read" })]);
  assert.deepEqual(
    rows.map((r) => [r.title, r.chapter, r.full]),
    [["The fix since you read", 0, false], ["Fog", 0, true], ["Ratify", 1, true]],
  );
  assert.equal(rows[0].note, "#7 · since you read");
  // The full brief may still be of the older head: its rows say so, and open it, written then.
  const stale = trayRows([pr(7, "Fog", { stale: true, writing: false, chapters: ["Fog"], quiet: false, since: "Since" })]);
  assert.deepEqual(stale.map((r) => [r.full, r.openable]), [[false, true], [true, true]]);
  assert.match(stale[1].note, /full brief · a newer push; rewritten when opened$/);
  // A story of chapters has one row each, so every chapter of it can be read to its end.
  const chaptered = trayRows([pr(7, "PR briefs", { stale: false, writing: false, chapters: ["Fog"], quiet: false, since: "The fix since you read", since_chapters: ["The read", "The fog"] })]);
  assert.deepEqual(
    chaptered.map((r) => [r.title, r.chapter, r.full]),
    [["The read", 0, false], ["The fog", 1, false], ["Fog", 0, true]],
  );
  assert.deepEqual(chaptered.slice(0, 2).map((r) => r.note), ["#7 The fix since you read · since you read · 1 of 2", "#7 The fix since you read · since you read · 2 of 2"]);
  // With no story, a row opens as before: the server writes the story first when the push calls for one.
  assert.deepEqual(trayRows([pr(7, "Fog", { stale: true, writing: false, chapters: ["Fog"], quiet: false })]).map((r) => r.full ?? false), [false]);
});

test("a short brief, made by code for a PR that changes the map but fogs nothing, is a row that says the map changed and opens", () => {
  const row = trayRows([pr(7, "Add beta", { stale: false, writing: false, chapters: ["Add beta"], quiet: false, short: true })])[0];
  assert.equal(row.openable, true);
  assert.equal(row.title, "Add beta");
  assert.equal(row.note, "#7 · map changed");
  // A push that carried the read over says so, as for any brief; a stale one says it is rewritten.
  const carried = trayRows([pr(7, "Add beta", { stale: false, writing: false, chapters: ["Add beta"], quiet: false, short: true, note: "New push touched nothing on your map" })])[0];
  assert.equal(carried.note, "#7 · New push touched nothing on your map");
  const stale = trayRows([pr(7, "Add beta", { stale: true, writing: false, chapters: ["Add beta"], quiet: false, short: true })])[0];
  assert.match(stale.note, /newer push/);
});
