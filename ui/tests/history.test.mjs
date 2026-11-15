// History (#94, spec #90): the ended sessions newest first, the search over name, project and branch, idle staying
// in the list and ended leaving it. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { branchOf, endedSessions, historyRows, matches, projectName } from "../../dist/history.js";
import { sidebarSessions } from "../../dist/list.js";
import { windowBinding } from "../../dist/keys.js";
import { initial } from "../../dist/settings.js";

const session = (id, state, last_event_at, extra = {}) => ({
  id, state, last_event_at, name: null, prompt_name: null, cwd: `/w/plotkeeper`, folder_name: "plotkeeper",
  identity: { issue: null, branch: null, pr: null, worktree: "", name: id }, transcript: { branch: null }, ...extra,
});
const ids = (list) => list.map((s) => s.id);

test("History holds the ended sessions, newest first; idle and your-turn sessions are not in it", () => {
  const all = [session("a", "ended", 10), session("b", "idle", 50), session("c", "ended", 30), session("d", "your_turn", 40), session("e", "ended", 20)];
  assert.deepEqual(ids(endedSessions(all)), ["c", "e", "a"]);
});

test("the session list keeps idle and your-turn sessions and drops the ended ones", () => {
  const all = [session("a", "needs_you", 5), session("b", "idle", 6), session("c", "ended", 7), session("d", "your_turn", 8)];
  assert.deepEqual(ids(sidebarSessions(all)), ["a", "b", "d"]);
});

test("a session that starts again is no longer ended, so it leaves History", () => {
  assert.deepEqual(ids(endedSessions([session("a", "running", 10)])), []);
});

test("the project is the repository's name, from a task's worktree too, else the folder's", () => {
  assert.equal(projectName({ cwd: "/w/Falinks/.claude/worktrees/issue-9-x", folder_name: "issue-9-x" }), "Falinks");
  assert.equal(projectName({ cwd: "/w/plotkeeper", folder_name: "plotkeeper" }), "plotkeeper");
});

test("the search matches name, project and branch, as a case-insensitive substring", () => {
  const a = session("a", "ended", 1, { name: "Fix the Parser", cwd: "/w/Falinks", folder_name: "Falinks", transcript: { branch: "feature/Login" } });
  assert.equal(branchOf(a), "feature/Login");
  for (const q of ["parser", "FALINKS", "login", " fix "]) assert.equal(matches(a, q), true, q);
  for (const q of ["plotkeeper", "nothing"]) assert.equal(matches(a, q), false, q);
  assert.equal(matches(a, ""), true);
});

test("the rows are the ended sessions that match, newest first", () => {
  const all = [
    session("a", "ended", 10, { name: "alpha" }),
    session("b", "ended", 30, { name: "alpha two" }),
    session("c", "idle", 40, { name: "alpha idle" }),
    session("d", "ended", 20, { name: "beta" }),
  ];
  assert.deepEqual(ids(historyRows(all, "ALPHA")), ["b", "a"]);
  assert.deepEqual(ids(historyRows(all, "")), ["b", "d", "a"]);
});

test("Show history is a palette action with no default key", () => {
  assert.equal(Object.values(initial.keymap.Workspace).some((b) => b === "studio::ShowHistory"), false);
  assert.equal(windowBinding(initial.keymap, { key: "y", code: "KeyY", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true }, false), undefined);
});
