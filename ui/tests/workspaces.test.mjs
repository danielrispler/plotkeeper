// Workspaces (danielrispler/Falinks#576) and the Git line (#29): a folded section at the bottom of the list, one row per workspace with
// its git tokens and session count, offering a new session and, for a task, Remove. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { branchBlocked, branchOptions, fetchNote, gitTokens, workspaceRows } from "../../dist/workspaces.js";
import { throttle } from "../../dist/throttle.js";

const ui = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, ui), "utf8");

const workspaces = {
  main: { path: "/r", branch: "main" },
  tasks: [
    { path: "/r/.claude/worktrees/issue-1", branch: "issue-1" },
    { path: "/r/.claude/worktrees/look", branch: null },
  ],
  hidden: 0,
};
const states = [
  { branch: "main", upstream: [0, 0], changed: 0, main_behind: null, on_default: false, would_conflict: false, operation: null },
  { branch: "issue-1", upstream: [2, 1], changed: 3 },
  { branch: null, upstream: null, changed: 0, main_behind: null, on_default: false, would_conflict: false, operation: null },
];
const sessions = ["/r", "/r/.claude/worktrees/issue-1", "/r/.claude/worktrees/issue-1/apps", "/elsewhere"].map((cwd) => ({ cwd }));
const texts = (state) => gitTokens(state).map((t) => t.text);

test("each workspace has its name, its git tokens and its session count, and offers a new session and, for a task, Remove", () => {
  const rows = workspaceRows(workspaces, states, sessions);
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

test("tokens that say nothing are hidden, and the rest come in order: branch, upstream, uncommitted", () => {
  assert.deepEqual(texts({ branch: "main", upstream: [0, 0], changed: 0, main_behind: null, on_default: false, would_conflict: false, operation: null }), ["main"]);
  assert.deepEqual(texts({ branch: "t", upstream: [0, 4], changed: 1 }), ["t", "↑0 ↓4", "●1"]);
  assert.deepEqual(texts({ branch: "t", upstream: [2, 0], changed: 0, main_behind: null, on_default: false, would_conflict: false, operation: null }), ["t", "↑2 ↓0"]);
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
  const state = { branch: "t", upstream: [0, 0], changed: 2, main_behind: null, on_default: false, would_conflict: false };
  const last = gitTokens({ ...state, operation: "rebase" }).at(-1);
  assert.equal(last.text, "⚠ rebase");
  assert.equal(last.title, "A rebase is in progress");
  assert.equal(gitTokens({ ...state, operation: "conflicts" }).at(-1).title, "Files have unresolved conflicts");
  assert.ok(!texts({ ...state, operation: null }).some((t) => t.includes("⚠")));
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

test("main ↓N shows only when behind, after the upstream distance and before the uncommitted count", () => {
  const state = { branch: "t", upstream: [2, 0], changed: 1, on_default: false, would_conflict: false, operation: null };
  assert.deepEqual(texts({ ...state, main_behind: 5 }), ["t", "↑2 ↓0", "main ↓5", "●1"]);
  assert.deepEqual(texts({ ...state, main_behind: null }), ["t", "↑2 ↓0", "●1"]);
  assert.equal(gitTokens({ ...state, main_behind: 5 })[2].title, "5 commits behind origin's default branch");
});

test("main ↓N is marked, and says why in words, when rebasing onto it would conflict", () => {
  const state = { branch: "t", upstream: [2, 0], changed: 0, on_default: false, would_conflict: false, operation: null, main_behind: 5 };
  assert.equal(gitTokens(state)[2].bad, false);
  const token = gitTokens({ ...state, would_conflict: true })[2];
  assert.equal(token.bad, true);
  assert.match(token.title, /rebasing onto it would conflict/);
});

test("on the default branch the distance shows once, as the upstream distance", () => {
  const main = { branch: "main", upstream: [0, 3], changed: 0, main_behind: 3, on_default: true, would_conflict: false, operation: null };
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
  assert.match(read("workspaces.ts"), /document\.hasFocus\(\) && void fetchNow\(\)[^]*5 \* 60_000/);
});

test("a task's row shows the landed mark when a session in it has landed; the main checkout never does", () => {
  const landed = [{ cwd: "/r", landed: true }, { cwd: "/r/.claude/worktrees/issue-1/apps", landed: true }];
  assert.deepEqual(workspaceRows(workspaces, states, landed).map((row) => row.landed), [false, true, false]);
  assert.deepEqual(workspaceRows(workspaces, states, sessions).map((row) => row.landed), [false, false, false]);
});

// The branch picker (#95): disabled while a session in the main checkout runs or needs Daniel.
test("the branch picker is disabled, with the reason, while a main-checkout session is running or needs you", () => {
  const at = (cwd, state) => ({ cwd, state });
  assert.equal(branchBlocked(workspaces, []), null);
  assert.equal(branchBlocked(workspaces, [at("/r", "idle"), at("/r", "your_turn"), at("/r", "ended")]), null);
  assert.equal(branchBlocked(workspaces, [at("/r", "running")]), "a session here is running");
  assert.equal(branchBlocked(workspaces, [at("/r/apps", "running"), at("/r", "needs_you")]), "a session here needs you");
});

test("a session in a task, or in another folder, does not disable the branch picker", () => {
  const at = (cwd, state) => ({ cwd, state });
  const others = [at("/r/.claude/worktrees/issue-1", "running"), at("/r/.claude/worktrees/issue-1/apps", "needs_you"), at("/elsewhere", "running")];
  assert.equal(branchBlocked(workspaces, others), null);
});

test("the branch options grey out a task's branch as 'in task X' and the current one, and all of them when blocked", () => {
  const branches = [
    { name: "main", remote: false, task: null, current: true },
    { name: "issue-1", remote: false, task: "issue-1", current: false },
    { name: "teammate", remote: true, task: null, current: false },
  ];
  const shown = (blocked) => branchOptions(branches, blocked).map(({ label, note, disabled }) => [label, note, disabled]);
  assert.deepEqual(shown(null), [
    ["main", "current", true],
    ["issue-1", "in task issue-1", true],
    ["teammate", "at origin", false],
  ]);
  assert.deepEqual(shown("a session here is running").map((row) => row[2]), [true, true, true]);
});

test("the picker is the main checkout's only: its token and the palette action open it, and it has no New branch", () => {
  assert.match(read("main.ts"), /"studio::SwitchBranch": \(\) => void showBranches\(\)/);
  assert.match(read("workspaces.ts"), /workspace !== read\?\.main/);
  assert.doesNotMatch(read("picker.ts"), /New branch/);
});
