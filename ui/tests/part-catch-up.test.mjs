// The part catch-up on a part's sheet (spec danielrispler/plotkeeper#378, ticket #391): pure logic, run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { awaitedCatchUp, changesLine, fogActions, fogListLines, olderLine, stageLabel } from "../../dist/map-catch-up.js";

/** @returns {import("../../dist/types.js").CatchUpOffer} */
const catchUp = (extra = {}) => ({ kind: "catch-up", ready: false, writing: false, failed: false, ...extra });

test("a fogged part's sheet shows its part catch-up as the main button and \"I know this\" as the second", () => {
  const actions = fogActions("thin", catchUp());
  assert.equal(actions.main.label, "Catch up");
  assert.deepEqual(actions.main.open, { kind: "catch-up" });
  assert.equal(actions.second, "I know this");
});

// The catch-up card (#636): a ready catch-up's button says how many steps its outline has; the card's eyebrow how many changes landed.
const outline = (n) => ({ title: "Store writes go atomic", steps: ["Where", "Before", "Now", "Must stay true"].slice(0, n).map((k) => ({ k, h: `${k} line` })) });

test("a ready catch-up's main button says how many steps it has, from its outline", () => {
  assert.equal(fogActions("thin", catchUp({ ready: true, outline: outline(4) })).main.label, "Catch up · 4 steps");
  assert.equal(fogActions("thin", catchUp({ ready: true, outline: outline(1) })).main.label, "Catch up · 1 step");
  assert.equal(fogActions("thin", catchUp({ ready: true, outline: outline(3) })).main.note, null);
});

test("a catch-up being written says the call that runs now on the card, and a failed one says to try again", () => {
  assert.equal(fogActions("thin", catchUp({ writing: true, stage: { kind: "checking" } })).main.note, "Checking claims…");
  assert.equal(fogActions("thin", catchUp({ writing: true, stage: { kind: "fixing", claims: 2 } })).main.label, "Catch up");
  assert.equal(fogActions("thin", catchUp({ failed: true })).main.note, "The last write failed; press to try again");
});

test("the card's eyebrow says how many changes landed since the last catch-up", () => {
  assert.equal(changesLine(3), "3 changes since you last caught up");
  assert.equal(changesLine(1), "1 change since you last caught up");
});

// The older catch-up (#638): what "Read the one you have" says it covers, of the part's fog entries, oldest first.
test("the older catch-up's line says how many of the part's changes it covers and names each, a PR by its number, a push by its commit", () => {
  const fogged = (commit, pr) => ({ commit, strength: "thin", pr, why: ["paths"] });
  const entries = [fogged("a".repeat(40), 612), fogged("b".repeat(40), null), fogged("c".repeat(40), 630)];
  assert.equal(olderLine({ outline: outline(3), fog: ["a".repeat(40), "b".repeat(40)] }, entries), "Covers 2 of 3 changes: #612, bbbbbbb");
  assert.equal(olderLine({ outline: outline(3), fog: ["a".repeat(40)] }, entries.slice(0, 2)), "Covers 1 of 2 changes: #612");
});

test("a part fogged by one PR whose brief lights it offers that chapter instead, with \"I know this\" second", () => {
  const actions = fogActions("thick", { kind: "chapter", pr: 7, chapter: 1 });
  assert.match(actions.main.label, /#7/);
  assert.deepEqual(actions.main.open, { kind: "chapter", pr: 7, chapter: 1 });
  assert.equal(actions.second, "I know this");
});

test("a clear part offers neither, and a fogged part the server has not judged still offers \"I know this\"", () => {
  assert.deepEqual(fogActions(undefined, catchUp()), { main: null, second: null });
  assert.deepEqual(fogActions("thin", undefined), { main: null, second: "I know this" });
});

test("\"Writing…\" never blocks: the button stays pressable while the catch-up is written, and the open waits for it", () => {
  const writing = fogActions("thin", catchUp({ writing: true }));
  assert.equal(writing.main.disabled, false);
  assert.equal(writing.main.note, "Writing…");
  assert.equal(writing.second, "I know this");
  assert.equal(awaitedCatchUp(catchUp({ writing: true })), "wait");
  assert.equal(awaitedCatchUp(catchUp()), "stop", "neither stored nor being written: nothing is on its way");
  assert.equal(awaitedCatchUp(catchUp({ ready: true })), "open");
  assert.equal(awaitedCatchUp(catchUp({ failed: true })), "stop");
  assert.equal(awaitedCatchUp(undefined), "stop", "the part was cleared meanwhile");
  assert.match(fogActions("thin", catchUp({ failed: true })).main.note, /failed/i);
});

test("the waiting card says which call of the write runs, each label ending in an ellipsis (#450)", () => {
  assert.equal(stageLabel(undefined), "Writing…", "before the server reports a stage");
  assert.equal(stageLabel({ kind: "writing" }), "Writing…");
  assert.equal(stageLabel({ kind: "checking" }), "Checking claims…");
  assert.equal(stageLabel({ kind: "fixing", claims: 3 }), "Fixing 3 problems…");
  assert.equal(stageLabel({ kind: "fixing", claims: 1 }), "Fixing 1 problem…");
});

test("a long catch-up's full fog list names each PR by number and each direct push by its subject and commit, oldest first", () => {
  const lines = fogListLines([
    { commit: "0123456789abcdef", pr: 7, title: "Keep the order" },
    { commit: "fedcba9876543210", pr: null, title: "fix pushed to main" },
  ]);
  assert.deepEqual(lines, ["#7 Keep the order", "fix pushed to main (fedcba9, no PR)"]);
});

test("a catch-up rewritten for a new fog list starts at its first step, while the same one reopens where it was left", async () => {
  const { briefPlayer } = await import("../../dist/map-brief-model.js");
  const { catchUpIds } = await import("../../dist/map-catch-up.js");
  const id = catchUpIds();
  const player = briefPlayer({ scrubbed: () => false, backToNow: () => {} });
  const old = id("/p", "a", ["c1"]);
  assert.ok(old < 0, "no PR has its number");
  player.open("/p", old, 0);
  player.go(1, 4);
  player.go(1, 4);
  player.go(1, 4);
  player.close();
  player.open("/p", id("/p", "a", ["c1"]), 0);
  assert.equal(player.current().step, 3, "the same catch-up reopens at the step left");
  player.close();
  player.open("/p", id("/p", "a", ["c1", "c2"]), 0);
  assert.equal(player.current().step, 0, "the rewritten one starts over");
  assert.notEqual(id("/p", "b", ["c1"]), old, "another part's has its own");
});
