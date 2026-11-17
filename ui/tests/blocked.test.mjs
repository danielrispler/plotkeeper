// A blocked command's card (danielrispler/plotkeeper#170, #171): its text and buttons by status. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { blockedView } from "../../dist/blocked.js";

const card = (status = "blocked", agent = null) => ({
  call: "toolu_1",
  command: 'claude -p "summarise" > /tmp/out.txt',
  cwd: "/work/repo",
  reason: "Nested agent runs are blocked.",
  agent,
  at: 1,
  status,
});

test("a blocked command shows in full with its folder and rule, and Allow and Decline, also after its hook has returned", () => {
  assert.deepEqual(blockedView(card(), null, true), {
    state: "Blocked",
    command: 'claude -p "summarise" > /tmp/out.txt',
    folder: "/work/repo",
    reason: "Nested agent runs are blocked.",
    agent: null,
    buttons: ["Allow", "Decline"],
    detail: null,
    tail: null,
  });
});

test("a subagent's block names the subagent", () => {
  assert.equal(blockedView(card("blocked", "a-7aaaa"), "Reviewer", true).agent, "Reviewer");
});

test("with no request file there is nothing to answer", () => {
  assert.deepEqual(blockedView(card(), null, false).buttons, []);
});

test("an allowed command waits for the agent's retry with no buttons, and a declined one is closed", () => {
  const allowed = blockedView(card("allowed"), null, true);
  assert.equal(allowed.state, "Allowed");
  assert.deepEqual(allowed.buttons, []);
  assert.equal(blockedView(card("declined"), null, true), null);
});

test("the card has no edit: it offers only Allow and Decline", () => {
  const all = ["blocked", "allowed"].flatMap((s) => blockedView(card(s), null, true).buttons);
  assert.deepEqual(all.filter((b) => !["Allow", "Decline"].includes(b)), []);
});

test("a running command shows the time since its retry started", () => {
  const view = blockedView({ ...card("running"), run: "toolu_2", run_at: 10_000 }, null, false, 75_000);
  assert.equal(view.state, "Running");
  assert.equal(view.detail, "1m");
  assert.deepEqual(view.buttons, []);
});

test("a finished command shows its exit code and the tail of its output, as a failed check does", () => {
  const done = { ...card("finished"), run: "toolu_2", exit_code: 2 };
  const view = blockedView(done, null, false, 0, { exit_code: 2, tail: "line one\nboom" });
  assert.equal(view.state, "Finished");
  assert.equal(view.detail, "exit code 2");
  assert.equal(view.tail, "line one\nboom");
  assert.equal(blockedView(done, null, false, 0, null).tail, null);
  assert.equal(blockedView({ ...done, exit_code: 0 }, null, false, 0, null).detail, "exit code 0");
  assert.equal(blockedView({ ...done, exit_code: null }, null, false, 0, null).detail, "No exit code");
});

test("a command in the background reads so until its wake-up", () => {
  const view = blockedView({ ...card("running_in_background"), task: "bash-7" }, null, false, 0);
  assert.equal(view.state, "Running in background");
  assert.deepEqual(view.buttons, []);
});

test("a command blocked again offers Allow and Decline once more", () => {
  const view = blockedView(card("blocked_again"), null, true);
  assert.equal(view.state, "Blocked again");
  assert.deepEqual(view.buttons, ["Allow", "Decline"]);
});

import { noteKeys } from "../../dist/typing.js";
import { retryLine, typeRetry } from "../../dist/blocked.js";

test("a command allowed but not run says so and offers Ask again, and nothing else", () => {
  const view = blockedView(card("allowed_not_run"), null, false);
  assert.equal(view.state, "Allowed, not run");
  assert.deepEqual(view.buttons, ["Ask again"]);
});

test("the session's own block is retried with a line that names the command", () => {
  assert.equal(
    retryLine(card(), null),
    'Daniel approved the blocked command: `claude -p "summarise" > /tmp/out.txt`. Run it again.',
  );
});

test("a subagent's block tells the session to cd into its folder in its own call, then run the exact command", () => {
  assert.equal(
    retryLine(card("blocked", "a-7aaaa"), "Reviewer"),
    'Daniel approved the command subagent `Reviewer` was blocked on. Run `cd /work/repo` as its own Bash call, then run exactly: `claude -p "summarise" > /tmp/out.txt`. Or pass it to the subagent if it still runs.',
  );
});

// A fake clock and terminal, as in typing.test.mjs.
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

test("the retry line goes in as a bracketed paste and Enter follows it", async () => {
  const { io, sent } = fake();
  const typed = await typeRetry(card(), null, "t-retry", () => false, io);
  assert.equal(typed, true);
  assert.equal(sent[0][1], `\x1b[200~${retryLine(card(), null)}\x1b[201~`);
  assert.equal(sent[1][1], "\r");
  assert.ok(sent[1][0] > sent[0][0]);
});

test("the retry line waits for Daniel's draft and is never typed into a session that needs him", async () => {
  noteKeys("t-draft", "half a thought", 0);
  const drafted = fake();
  assert.equal(await typeRetry(card(), null, "t-draft", () => false, drafted.io), false);
  assert.deepEqual(drafted.sent, []);
  assert.match(drafted.notices[0], /draft/);

  const needing = fake();
  assert.equal(await typeRetry(card(), null, "t-needs", () => true, needing.io), false);
  assert.deepEqual(needing.sent, []);
  assert.equal(needing.notices.length, 1);
});

import { blockedRequestFor, sendBlockedAnswer, startWatching } from "../../dist/answer.js";

// The shell as `invoke` and `listen` see it: `answers` is what `answer_blocked_command` returns, or an error to throw.
function shell(answers) {
  const handlers = {};
  globalThis.document = { addEventListener() {}, visibilityState: "hidden" };
  globalThis.setInterval = () => 0;
  globalThis.window = {
    __TAURI__: {
      core: {
        invoke: async (command) => {
          if (command !== "answer_blocked_command") return null;
          const next = answers.shift();
          if (next instanceof Error) throw next;
          return next;
        },
      },
      event: { listen: async (name, handler) => void (handlers[name] = (payload) => handler({ payload })) },
    },
  };
  startWatching(() => {});
  return handlers;
}

test("the answer says whether the hook took it or it was late, which decides whether the retry line is typed", async () => {
  const handlers = shell(["hook", "late"]);
  const request = (nonce) => ({ nonce, tool_use_id: `toolu_${nonce}`, waiting: true });
  handlers.blocked([request("n1"), request("n2")]);
  assert.equal(await sendBlockedAnswer("s1", request("n1"), "allow"), "hook");
  assert.equal(await sendBlockedAnswer("s1", request("n2"), "allow"), "late");
});

test("a failed answer puts the request back so the card keeps its buttons", async () => {
  const handlers = shell([new Error("no such request")]);
  const original = { nonce: "n1", tool_use_id: "toolu_n1", waiting: true };
  handlers.blocked([original]);
  const origWarn = console.warn;
  console.warn = () => {};
  const took = await sendBlockedAnswer("s1", original, "allow");
  console.warn = origWarn;
  assert.equal(took, null);
  assert.deepEqual(blockedRequestFor("toolu_n1"), original);
});
