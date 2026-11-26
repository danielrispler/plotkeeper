// The projects the page groups its sessions by (#97, #326): a session's project folder, the core's (spec #659), never derived from its
// folder on the page. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { knownProjects, projectGroups, selectedProject, sessionProjectName, sidebarOrder } from "../../dist/projects.js";
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

test("a saved project with no live session keeps its header, after the projects with sessions", () => {
  const saved = [
    { path: "/r/quiet", gone: false, temporary: false },
    { path: "/r/p", gone: false, temporary: false },
  ];
  const groups = projectGroups([at("a", "running", "/r/p"), at("b", "ended", "/r/quiet"), at("c", "idle", "/r/q")], saved);
  assert.deepEqual(
    groups.map((g) => [g.key, g.states.length, g.gone]),
    [["/r/p", 1, false], ["/r/q", 1, false], ["/r/quiet", 0, false]],
  );
});

// Spec #659: a gone or temporary saved project shows only while it has a live session, and the server still lists it, so a gone one
// comes back with its folder.
test("a gone or temporary saved project is left out unless it has a live session, and a gone one with one says so", () => {
  const saved = [
    { path: "/r/moved", gone: true, temporary: false },
    { path: "/private/tmp/tmp.x1", gone: false, temporary: true },
    { path: "/r/gone-live", gone: true, temporary: false },
    { path: "/tmp/trial", gone: false, temporary: true },
    { path: "/r/quiet", gone: false, temporary: false },
  ];
  const sessions = [at("a", "running", "/r/gone-live"), at("b", "idle", "/tmp/trial"), at("c", "ended", "/r/moved"), at("d", "ended", "/private/tmp/tmp.x1")];
  assert.deepEqual(
    projectGroups(sessions, saved).map((g) => [g.key, g.gone]),
    [["/r/gone-live", true], ["/tmp/trial", false], ["/r/quiet", false]],
  );
  // A session Clear landed hid still makes its project live, so a gone one keeps its header.
  assert.deepEqual(projectGroups([at("a", "idle", "/r/moved")], saved, new Set(["a"])).map((g) => g.key), ["/r/moved", "/r/quiet"]);
  // Its folder back, the gone project shows again.
  assert.deepEqual(projectGroups([], [{ path: "/r/moved", gone: false, temporary: false }]).map((g) => g.key), ["/r/moved"]);
});

// #380's review: Clear landed hides a project's last live sessions from the list, but they are still live, and the server saves the project
// again on their next event, so the project still counts as live: its header offers no Remove project.
test("a project whose only live sessions Clear landed hid shows no sessions and is still live; a saved one with none is not", () => {
  const saved = [{ path: "/r/p", gone: false, temporary: false }, { path: "/r/quiet", gone: false, temporary: false }];
  const groups = projectGroups([at("a", "idle", "/r/p"), at("b", "running", "/r/p/.claude/worktrees/x"), at("c", "ended", "/r/quiet")], saved, new Set(["a", "b"]));
  assert.deepEqual(
    groups.map((g) => [g.key, g.states.length, g.live]),
    [["/r/p", 0, true], ["/r/quiet", 0, false]],
  );
  assert.equal(projectGroups([at("a", "idle", "/r/p")])[0].live, true);
});

test("the Map view's projects are the saved ones, and the selected session's is its own", () => {
  studio.projects = [{ path: "/r/p", gone: false, temporary: false }, { path: "/r/gone", gone: true, temporary: false }];
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
