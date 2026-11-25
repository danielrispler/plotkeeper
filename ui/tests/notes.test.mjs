// The note toast and the unseen chip, on `Session` JSON. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { finishedNote, noteFade } from "../../dist/notes.js";

const session = (id, rest = {}) => /** @type {import("../../dist/types.js").Session} */ ({
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


// A clock whose time moves by `tick`, running each timer that falls due.
function clock() {
  let now = 0;
  let next = 1;
  const timers = new Map();
  return {
    set(run, ms) {
      timers.set(next, { at: now + ms, run });
      return next++;
    },
    clear: (id) => timers.delete(id),
    tick(ms) {
      const end = now + ms;
      for (;;) {
        const due = [...timers].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
        if (!due) break;
        timers.delete(due[0]);
        now = due[1].at;
        due[1].run();
      }
      now = end;
    },
  };
}

test("the note fades 6 s after it shows", () => {
  const time = clock();
  let hidden = 0;
  const fade = noteFade(time, () => hidden++);
  fade.start();
  time.tick(5999);
  assert.equal(hidden, 0);
  time.tick(1);
  assert.equal(hidden, 1);
});

test("the note holds its fade while focused, and fades 6 s after focus leaves (#457)", () => {
  const time = clock();
  let hidden = 0;
  const fade = noteFade(time, () => hidden++);
  fade.start();
  time.tick(3000);
  fade.hold("focus");
  time.tick(20000);
  assert.equal(hidden, 0, "a focused note stays");
  fade.release("focus");
  time.tick(5999);
  assert.equal(hidden, 0);
  time.tick(1);
  assert.equal(hidden, 1);
});

test("the pointer leaving a focused note does not resume its fade, nor a blur under the pointer", () => {
  const time = clock();
  let hidden = 0;
  const fade = noteFade(time, () => hidden++);
  fade.start();
  fade.hold("focus");
  fade.hold("pointer");
  fade.release("pointer");
  time.tick(20000);
  assert.equal(hidden, 0, "still focused");
  fade.hold("pointer");
  fade.release("focus");
  time.tick(20000);
  assert.equal(hidden, 0, "still under the pointer");
  fade.release("pointer");
  time.tick(6000);
  assert.equal(hidden, 1);
});

test("a newer note shown while the note is focused keeps it held", () => {
  const time = clock();
  let hidden = 0;
  const fade = noteFade(time, () => hidden++);
  fade.start();
  fade.hold("focus");
  fade.start();
  time.tick(20000);
  assert.equal(hidden, 0);
});
