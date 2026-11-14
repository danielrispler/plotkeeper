// Places (danielrispler/Falinks#576) and the Git line (#29): a folded section at the bottom of the list, one row per place with
// its git tokens and session count, offering a new session and, for a task, Remove. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fetchNote, gitTokens, placeRows } from "../../dist/places.js";
import { throttle } from "../../dist/throttle.js";

const ui = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, ui), "utf8");

const places = {
  main: { path: "/r", branch: "main" },
  tasks: [
    { path: "/r/.claude/worktrees/issue-1", branch: "issue-1" },
    { path: "/r/.claude/worktrees/look", branch: null },
  ],
  hidden: 0,
};
const states = [
  { branch: "main", upstream: [0, 0], changed: 0, main_behind: null, on_default: false },
  { branch: "issue-1", upstream: [2, 1], changed: 3 },
  { branch: null, upstream: null, changed: 0, main_behind: null, on_default: false },
];
const sessions = ["/r", "/r/.claude/worktrees/issue-1", "/r/.claude/worktrees/issue-1/apps", "/elsewhere"].map((cwd) => ({ cwd }));
const texts = (state) => gitTokens(state).map((t) => t.text);

test("Places is folded", () => {
  const html = read("index.html").match(/<details id="places"[^>]*>/)[0];
  assert.doesNotMatch(html, /\sopen/);
});

test("each place has its name, its git tokens and its session count, and offers a new session and, for a task, Remove", () => {
  const rows = placeRows(places, states, sessions);
  assert.deepEqual(
    rows.map(({ name, facts, actions }) => [name, facts, actions]),
    [
      ["Main checkout", "main  1 session", ["New session here"]],
      // A session in a task's subfolder counts for the task, not the main checkout around it.
      ["issue-1", "issue-1  ↑2 ↓1  ●3  2 sessions", ["New session here", "Remove"]],
      ["look (detached)", "detached", ["New session here", "Remove"]],
    ],
  );
});

test("a row shows the same tokens as the Git line for the same place", () => {
  const rows = placeRows(places, states, []);
  rows.forEach((row, i) => assert.equal(row.facts, texts(states[i]).join("  ")));
});

test("tokens that say nothing are hidden, and the rest come in order: branch, upstream, uncommitted", () => {
  assert.deepEqual(texts({ branch: "main", upstream: [0, 0], changed: 0, main_behind: null, on_default: false }), ["main"]);
  assert.deepEqual(texts({ branch: "t", upstream: [0, 4], changed: 1 }), ["t", "↑0 ↓4", "●1"]);
  assert.deepEqual(texts({ branch: "t", upstream: [2, 0], changed: 0, main_behind: null, on_default: false }), ["t", "↑2 ↓0"]);
});

test("a branch with no upstream says not pushed; a detached HEAD says detached and not not pushed", () => {
  assert.deepEqual(texts({ branch: "t", upstream: null, changed: 2 }), ["t", "not pushed", "●2"]);
  assert.deepEqual(texts({ branch: null, upstream: null, changed: 2 }), ["detached", "●2"]);
  assert.deepEqual(texts(null), []);
});

test("the hover says each count in words", () => {
  const titles = gitTokens({ branch: "t", upstream: [2, 1], changed: 3 }).map((t) => t.title);
  assert.deepEqual(titles, ["On branch t", "2 commits not pushed, 1 commit not pulled", "3 uncommitted files"]);
  assert.equal(gitTokens({ branch: null, upstream: null, changed: 1 })[0].title, "Detached HEAD");
  assert.equal(gitTokens({ branch: "t", upstream: null, changed: 1 })[1].title, "The branch has no upstream: never pushed");
});

test("an operation in progress shows last as a warning, and is hidden with none", () => {
  const state = { branch: "t", upstream: [0, 0], changed: 2 };
  const last = gitTokens({ ...state, operation: "rebase" }).at(-1);
  assert.equal(last.text, "⚠ rebase");
  assert.equal(last.title, "A rebase is in progress");
  assert.equal(gitTokens({ ...state, operation: "conflicts" }).at(-1).title, "Files have unresolved conflicts");
  for (const operation of [null, undefined]) assert.ok(!texts({ ...state, operation }).some((t) => t.includes("⚠")));
});

// A clock the test moves, and the timers set against it.
function fakeClock() {
  const timers = [];
  const now = { t: 0 };
  return {
    now,
    clock: () => now.t,
    schedule: (run, delay) => timers.push({ run, at: now.t + delay }),
    advance(to) {
      for (;;) {
        const due = timers.filter((x) => x.at <= to).sort((a, b) => a.at - b.at)[0];
        if (!due) break;
        timers.splice(timers.indexOf(due), 1);
        now.t = due.at;
        due.run();
      }
      now.t = to;
    },
  };
}

function reading() {
  const c = fakeClock();
  const reads = [];
  return { c, reads, event: throttle(() => reads.push(c.now.t), 2000, c.clock, c.schedule) };
}

test("throttle: the first event reads at once", () => {
  const { reads, event } = reading();
  event();
  assert.deepEqual(reads, [0]);
});

test("throttle: a burst reads once per 2 s, and one read follows the last event", () => {
  const { c, reads, event } = reading();
  for (let t = 0; t <= 4500; t += 500) {
    c.advance(t);
    event();
  }
  assert.deepEqual(reads, [0, 2000, 4000]);
  // The last event, at 4500, is read at 6000, 2 s after the read before it.
  c.advance(5999);
  assert.deepEqual(reads, [0, 2000, 4000]);
  c.advance(6000);
  assert.deepEqual(reads, [0, 2000, 4000, 6000]);
  c.advance(20000);
  assert.deepEqual(reads, [0, 2000, 4000, 6000]);
});

test("throttle: an event after a quiet spell reads at once", () => {
  const { c, reads, event } = reading();
  event();
  c.advance(5000);
  event();
  assert.deepEqual(reads, [0, 5000]);
});

test("the new-session picker no longer creates or removes tasks", () => {
  const picker = read("picker.ts");
  assert.doesNotMatch(picker, /create_task"|remove_task|Remove/);
  assert.match(read("places.ts"), /create_task"/);
});

test("main ↓N shows only when behind, after the upstream distance and before the uncommitted count", () => {
  const state = { branch: "t", upstream: [2, 0], changed: 1, on_default: false };
  assert.deepEqual(texts({ ...state, main_behind: 5 }), ["t", "↑2 ↓0", "main ↓5", "●1"]);
  assert.deepEqual(texts({ ...state, main_behind: null }), ["t", "↑2 ↓0", "●1"]);
  assert.equal(gitTokens({ ...state, main_behind: 5 })[2].title, "5 commits behind origin's default branch");
});

test("main ↓N is marked, and says why in words, when rebasing onto it would conflict", () => {
  const state = { branch: "t", upstream: [2, 0], changed: 0, on_default: false, main_behind: 5 };
  assert.equal(gitTokens(state)[2].bad, undefined);
  const token = gitTokens({ ...state, would_conflict: true })[2];
  assert.equal(token.bad, true);
  assert.match(token.title, /rebasing onto it would conflict/);
});

test("on the default branch the distance shows once, as the upstream distance", () => {
  const main = { branch: "main", upstream: [0, 3], changed: 0, main_behind: 3, on_default: true };
  assert.deepEqual(texts(main), ["main", "↑0 ↓3"]);
});

// The Git line's hover says how long ago the last successful fetch was, and that the latest failed (#33).
test("the fetch note: never, its age, and a failure with git's message", () => {
  const minute = 60_000;
  assert.equal(fetchNote({ at: null, error: null }, 0), "Not fetched yet");
  assert.equal(fetchNote({ at: 0, error: null }, 30_000), "Last fetch: just now");
  assert.equal(fetchNote({ at: 0, error: null }, 3 * minute), "Last fetch: 3 minutes ago");
  assert.equal(fetchNote({ at: 0, error: null }, 61 * minute), "Last fetch: 1 hour ago");
  assert.equal(
    fetchNote({ at: 0, error: "fatal: 'origin' does not appear to be a git repository" }, 3 * minute),
    "Last fetch: 3 minutes ago\nThe latest fetch failed: fatal: 'origin' does not appear to be a git repository",
  );
  assert.equal(fetchNote({ at: null, error: "offline" }, 0), "Not fetched yet\nThe latest fetch failed: offline");
});

test("Fetch is a palette action, and the 5-minute fetch runs only while the window is focused", () => {
  assert.match(read("main.ts"), /"studio::Fetch": \(\) =>/);
  assert.match(read("places.ts"), /document\.hasFocus\(\) && void fetchNow\(\)[^]*5 \* 60_000/);
});
