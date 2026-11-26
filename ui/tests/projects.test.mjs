// The projects the page groups its sessions by (#97, #326): a session's project folder, the core's (spec #659), never derived from its
// folder on the page. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { holdGroups, knownProjects, projectGroups, selectedProject, sessionProjectName, sidebarOrder } from "../../dist/projects.js";
import { studio } from "../../dist/state.js";

// A session with only the fields the projects read: its project, the core's, and its folder, which a `cd` moves. The project of a
// task's worktree is its main checkout, as the core sets it.
const at = (id, state, cwd, project = cwd.replace(/\/\.claude\/worktrees\/.*/, "")) => /** @type {import("../../dist/types.js").Session} */ ({ id, state, cwd, project });

test("a session's project name is its project's last segment, else ~", () => {
  assert.equal(sessionProjectName(at("a", "idle", "/r/plotkeeper/.claude/worktrees/x")), "plotkeeper");
  assert.equal(sessionProjectName(at("a", "idle", "/")), "~");
});

test("a session whose folder changed stays in the project it started in", () => {
  const groups = projectGroups([at("a", "running", "/r/plotkeeper/ui", "/r/plotkeeper"), at("b", "idle", "/r/other", "/r/plotkeeper"), at("c", "idle", "/r/plotkeeper")]);
  assert.deepEqual(groups.map((g) => [g.key, g.states.flatMap((s) => s.sessions.map((x) => x.id))]), [["/r/plotkeeper", ["a", "b", "c"]]]);
  assert.equal(sessionProjectName(at("a", "idle", "/r/plotkeeper/ui", "/r/plotkeeper")), "plotkeeper");
});

test("one group per repository in listing order, its sessions by state, ended ones left out", () => {
  const groups = projectGroups([
    at("a", "idle", "/r/plotkeeper"),
    at("b", "running", "/r/plotkeeper/.claude/worktrees/x"),
    at("c", "needs_you", "/r/api"),
    at("d", "ended", "/r/api"),
    at("e", "ended", "/r/old"),
  ]);
  assert.deepEqual(groups.map((g) => [g.key, g.name, g.rollup]), [["/r/plotkeeper", "plotkeeper", "running"], ["/r/api", "api", "needs_you"]]);
  assert.deepEqual(groups[0].states.map((s) => [s.state, s.sessions.map((x) => x.id)]), [["running", ["b"]], ["idle", ["a"]]]);
});

test("the sidebar's order is project by project, by state", () => {
  const sessions = [at("a", "idle", "/r/p"), at("b", "running", "/r/q"), at("c", "running", "/r/p"), at("d", "ended", "/r/p")];
  assert.deepEqual(sidebarOrder(sessions).map((s) => s.id), ["c", "a", "b"]);
});

test("a saved project with no live session keeps its header, after the projects with sessions, and a gone one says so", () => {
  const saved = [
    { path: "/r/quiet", gone: false },
    { path: "/r/p", gone: false },
    { path: "/r/moved", gone: true },
  ];
  const groups = projectGroups([at("a", "running", "/r/p"), at("b", "ended", "/r/quiet"), at("c", "idle", "/r/q")], saved);
  assert.deepEqual(
    groups.map((g) => [g.key, g.states.length, g.gone]),
    [["/r/p", 1, false], ["/r/q", 1, false], ["/r/quiet", 0, false], ["/r/moved", 0, true]],
  );
});

// #380's review: Clear landed hides a project's last live sessions from the list, but they are still live, and the server saves the project
// again on their next event, so the project still counts as live: its header offers no Remove project.
test("a project whose only live sessions Clear landed hid shows no sessions and is still live; a saved one with none is not", () => {
  const saved = [{ path: "/r/p", gone: false }, { path: "/r/quiet", gone: false }];
  const groups = projectGroups([at("a", "idle", "/r/p"), at("b", "running", "/r/p/.claude/worktrees/x"), at("c", "ended", "/r/quiet")], saved, new Set(["a", "b"]));
  assert.deepEqual(
    groups.map((g) => [g.key, g.states.length, g.live]),
    [["/r/p", 0, true], ["/r/quiet", 0, false]],
  );
  assert.equal(projectGroups([at("a", "idle", "/r/p")])[0].live, true);
});

test("the Map view's projects are the saved ones, and the selected session's is its own", () => {
  studio.projects = [{ path: "/r/p", gone: false }, { path: "/r/gone", gone: true }];
  studio.listed = [at("a", "idle", "/r/p/.claude/worktrees/x"), at("b", "ended", "/r/q")];
  assert.deepEqual(knownProjects(), ["/r/p", "/r/gone"]);
  studio.selection = null;
  assert.equal(selectedProject(), null);
  studio.selection = { session: studio.listed[0], agent: null };
  assert.equal(selectedProject(), "/r/p");
  // A selected session that moved to another folder still opens its project's map.
  studio.selection = { session: at("c", "idle", "/r/q/ui", "/r/p"), agent: null };
  assert.equal(selectedProject(), "/r/p");
});

// The held order (spec #659): while the pointer is in the list, the groups and the rows in each keep the last draw's places.
const shown = (groups) => groups.map((g) => [g.key, g.states.map((s) => [s.state, s.sessions.map((x) => [x.id, x.state])])]);

test("while held, a session whose state changed keeps its place, and so does its group, with its new state", () => {
  const last = projectGroups([at("a", "needs_you", "/r/p"), at("b", "running", "/r/p"), at("c", "running", "/r/q"), at("d", "idle", "/r/q")]);
  // a's question was answered and c now needs you: unheld, q's group would go first and c before d's idle group's row.
  const now = projectGroups([at("c", "needs_you", "/r/q"), at("b", "running", "/r/p"), at("a", "running", "/r/p"), at("d", "your_turn", "/r/q")]);
  assert.deepEqual(shown(holdGroups(last, now)), [
    ["/r/p", [["needs_you", [["a", "running"]]], ["running", [["b", "running"]]]]],
    ["/r/q", [["running", [["c", "needs_you"]]], ["idle", [["d", "your_turn"]]]]],
  ]);
});

test("while held, a session no longer listed leaves, and a group with nothing left goes unless it is a saved header", () => {
  const saved = [{ path: "/r/s", gone: false }];
  const last = projectGroups([at("a", "running", "/r/p"), at("b", "running", "/r/p"), at("c", "idle", "/r/q")], saved);
  // b was deleted, c ended: q's group has no live session and is not saved, so it goes; the saved header stays.
  const now = projectGroups([at("a", "running", "/r/p"), at("c", "ended", "/r/q")], saved);
  assert.deepEqual(shown(holdGroups(last, now)), [["/r/p", [["running", [["a", "running"]]]]], ["/r/s", []]]);
});

test("while held, a new session joins the end of its project's group, and a new project the end of the groups", () => {
  const last = projectGroups([at("a", "running", "/r/p"), at("b", "idle", "/r/p"), at("c", "running", "/r/q")]);
  // d and e start in p, needing you and idle; f starts the new project n, which unheld would go first. The new rows come after the
  // held ones in the order they are listed, each in its own state group unless it shares the last one's state.
  const now = projectGroups([at("f", "needs_you", "/r/n"), at("d", "needs_you", "/r/p"), at("a", "running", "/r/p"), at("c", "running", "/r/q"), at("b", "idle", "/r/p"), at("e", "idle", "/r/p")]);
  assert.deepEqual(shown(holdGroups(last, now)), [
    ["/r/p", [["running", [["a", "running"]]], ["idle", [["b", "idle"]]], ["needs_you", [["d", "needs_you"]]], ["idle", [["e", "idle"]]]]],
    ["/r/q", [["running", [["c", "running"]]]]],
    ["/r/n", [["needs_you", [["f", "needs_you"]]]]],
  ]);
});
