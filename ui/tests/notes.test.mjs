// The note toast, the context warning and the unseen chip, on `Session` JSON. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { contextNote, crossings, finishedNote } from "../../dist/notes.js";
import { pastWarning, unseenText } from "../../dist/list.js";

const session = (id, share, rest = {}) => ({
  id,
  name: id,
  state: "running",
  identity: { issue: null, name: id },
  folder_name: id,
  transcript: { intention: `${id} did it`, context: share === null ? null : { tokens: 1000, window: 200000, share, left: 100 - share } },
  ...rest,
});

test("a share past the threshold warns, not one at it, and 0 never warns", () => {
  assert.equal(pastWarning({ share: 31 }, 30), true);
  assert.equal(pastWarning({ share: 30 }, 30), false);
  assert.equal(pastWarning({ share: 99 }, 0), false);
  assert.equal(pastWarning({ share: null }, 30), false);
  assert.equal(pastWarning(null, 30), false);
});

test("a session warns once as it passes the threshold, and again only after falling back", () => {
  const warned = new Set();
  const at = (share) => crossings([session("C", share)], 30, warned).map((s) => s.id);
  assert.deepEqual(at(28), []);
  assert.deepEqual(at(31), ["C"]);
  assert.deepEqual(at(45), []);
  // `/compact` brings it back under, which re-arms it.
  assert.deepEqual(at(12), []);
  assert.deepEqual(at(31), ["C"]);
});

test("an unknown share keeps the session's place, and an ended session neither warns nor re-arms", () => {
  const warned = new Set(["C"]);
  assert.deepEqual(crossings([session("C", null)], 30, warned), []);
  assert.ok(warned.has("C"));
  assert.deepEqual(crossings([session("D", 50, { state: "ended" })], 30, warned), []);
  assert.ok(!warned.has("D"));
});

test("the context note names the first session, its share and how many passed", () => {
  const one = contextNote([session("Fix parser", 34)], 30);
  assert.equal(one.session.id, "Fix parser");
  assert.equal(one.line, "Context passed 30%: now 34%");
  assert.equal(one.count, "");
  assert.equal(contextNote([session("A", 34), session("B", 40)], 30).count, "2 past 30%");
  assert.equal(contextNote([], 30), null);
});

test("the finished note names the first listed session and its intention, and counts the rest", () => {
  const listed = [session("A", 5), session("B", 5), session("C", 5)];
  const one = finishedNote(["B"], listed);
  assert.equal(one.session.id, "B");
  assert.equal(one.line, "B did it");
  assert.equal(one.count, "");
  assert.equal(finishedNote(["C", "A", "gone"], listed).count, "2 finished");
  assert.equal(finishedNote(["C", "A"], listed).session.id, "C");
  assert.equal(finishedNote(["gone"], listed), null);
});

test("the unseen chip counts the unseen sessions, and says nothing with none", () => {
  assert.equal(unseenText([{ unseen: true }, { unseen: false }, { unseen: true }]), "2 unseen");
  assert.equal(unseenText([{ unseen: false }, {}]), "");
});
