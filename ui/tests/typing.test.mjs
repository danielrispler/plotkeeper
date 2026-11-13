// Typing into a pane (danielrispler/Falinks#544): the studio waits for Daniel's draft, never types into a session that
// needs him, and pastes before it presses Enter. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { noteKeys, typeInto } from "../../dist/typing.js";

// A fake clock and terminal: `sleep` moves the clock, and `keys` lands Daniel's keys at given times.
function fake(keys = []) {
  let now = 0;
  const sent = [];
  const notices = [];
  const pending = [...keys];
  const io = {
    now: () => now,
    sleep: async (ms) => {
      now += ms;
      while (pending.length && pending[0][0] <= now) {
        const [at, id, data] = pending.shift();
        noteKeys(id, data, at);
      }
    },
    send: async (data) => sent.push([now, data]),
    notice: (text) => notices.push(text),
  };
  return { io, sent, notices };
}

test("with a draft being typed, the rename waits, and types once the draft is sent", async () => {
  noteKeys("t1", "fix the", 0);
  const { io, sent } = fake([
    [1000, "t1", " parser"],
    [2000, "t1", "\r"],
  ]);

  const typed = await typeInto("t1", "/rename parser", () => false, io);

  assert.equal(typed, true);
  // Nothing while he types, and nothing in the 3 s after his last key.
  assert.ok(sent[0][0] >= 5000, `${sent[0][0]}`);
  assert.equal(sent[0][1], "\x1b[200~/rename parser\x1b[201~");
});

test("a draft cleared with backspaces counts as cleared", async () => {
  noteKeys("t2", "ab", 0);
  const { io, sent } = fake([[500, "t2", "\x7f\x7f"]]);

  assert.equal(await typeInto("t2", "/rename x", () => false, io), true);
  assert.equal(sent.length, 2);
});

test("a draft left in the prompt is never typed over: after 60 s a notice shows instead", async () => {
  noteKeys("t3", "half a thought", 0);
  const { io, sent, notices } = fake();

  const typed = await typeInto("t3", "/rename x", () => false, io);

  assert.equal(typed, false);
  assert.deepEqual(sent, []);
  assert.equal(notices.length, 1);
  assert.match(notices[0], /draft/);
});

test("a session in needs you gets no typing, and a notice shows", async () => {
  const { io, sent, notices } = fake();

  const typed = await typeInto("t4", "/rename x", () => true, io);

  assert.equal(typed, false);
  assert.deepEqual(sent, []);
  assert.equal(notices.length, 1);
  assert.match(notices[0], /needs you/);
});

test("the command arrives as a bracketed paste, then Enter 200 ms later", async () => {
  const { io, sent } = fake();

  assert.equal(await typeInto("t5", "/rename parser", () => false, io), true);

  assert.deepEqual(sent, [
    [0, "\x1b[200~/rename parser\x1b[201~"],
    [200, "\r"],
  ]);
});

test("a session that needs you between the paste and Enter gets no Enter, and a notice shows", async () => {
  const { io, sent, notices } = fake();
  let checks = 0;

  const typed = await typeInto("t6", "/rename x", () => checks++ > 0, io);

  assert.equal(typed, false);
  assert.deepEqual(sent, [[0, "\x1b[200~/rename x\x1b[201~"]]);
  assert.equal(notices.length, 1);
  assert.match(notices[0], /needs you/);
});

test("Option-Backspace takes a word off the draft", async () => {
  noteKeys("t7", "fix the", 0);
  noteKeys("t7", "\x1b\x7f", 100);
  const left = fake();
  assert.equal(await typeInto("t7", "/rename x", () => false, left.io), false);
  assert.match(left.notices[0], /draft/);

  noteKeys("t8", "fix the", 0);
  noteKeys("t8", "\x1b\x7f\x1b\x7f", 100);
  const cleared = fake();
  assert.equal(await typeInto("t8", "/rename x", () => false, cleared.io), true);
});
