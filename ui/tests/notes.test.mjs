// The note toast and the unseen chip, on `Session` JSON. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { finishedNote } from "../../dist/notes.js";

const session = (id, rest = {}) => ({
  id,
  name: id,
  state: "running",
  identity: { issue: null, name: id },
  folder_name: id,
  transcript: { intention: `${id} did it`, context: { tokens: 1000 } },
  ...rest,
});

test("the finished note names the first listed session and its intention, and counts the rest", () => {
  const listed = [session("A"), session("B"), session("C")];
  const one = finishedNote(["B"], listed);
  assert.equal(one.session.id, "B");
  assert.equal(one.line, "B did it");
  assert.equal(one.count, "");
  assert.equal(finishedNote(["C", "A", "gone"], listed).count, "2 finished");
  assert.equal(finishedNote(["C", "A"], listed).session.id, "C");
  assert.equal(finishedNote(["gone"], listed), null);
});

