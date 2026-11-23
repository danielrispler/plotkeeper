// The part catch-up on a part's sheet (spec danielrispler/plotkeeper#378, ticket #391): pure logic, run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { awaitedCatchUp, fogActions, fogListLines } from "../../dist/map-model.js";

const catchUp = (extra = {}) => ({ kind: "catch-up", ready: false, writing: false, failed: false, ...extra });

test("a fogged part's sheet shows its part catch-up as the main button and \"I know this\" as the second", () => {
  const actions = fogActions("thin", catchUp());
  assert.equal(actions.main.label, "Part catch-up");
  assert.deepEqual(actions.main.open, { kind: "catch-up" });
  assert.equal(actions.second, "I know this");
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
  assert.equal(awaitedCatchUp(catchUp()), "wait", "not written yet: its write is on its way");
  assert.equal(awaitedCatchUp(catchUp({ ready: true })), "open");
  assert.equal(awaitedCatchUp(catchUp({ failed: true })), "stop");
  assert.equal(awaitedCatchUp(undefined), "stop", "the part was cleared meanwhile");
  assert.match(fogActions("thin", catchUp({ failed: true })).main.note, /failed/i);
});

test("a long catch-up's full fog list names each PR by number and each direct push by its subject and commit, oldest first", () => {
  const lines = fogListLines([
    { commit: "0123456789abcdef", pr: 7, title: "Keep the order" },
    { commit: "fedcba9876543210", pr: null, title: "fix pushed to main" },
  ]);
  assert.deepEqual(lines, ["#7 Keep the order", "fix pushed to main (fedcba9, no PR)"]);
});
