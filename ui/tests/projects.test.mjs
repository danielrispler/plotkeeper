// The projects the page groups its sessions by (#97, #326): a repository some session ran in, a task's worktree in its main
// checkout's project. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { knownProjects, projectGroups, projectOfCwd, repositoryOf, selectedProject, sessionProjectName, sidebarOrder } from "../../dist/projects.js";
import { studio } from "../../dist/state.js";

// A session with only the fields the projects read.
const at = (id, state, cwd) => /** @type {import("../../dist/types.js").Session} */ ({ id, state, cwd });

test("a task's worktree belongs to its main checkout's project, any other folder to its own", () => {
  assert.equal(repositoryOf("/r/plotkeeper/.claude/worktrees/issue-1/ui"), "/r/plotkeeper");
  assert.equal(repositoryOf("/r/plotkeeper"), "/r/plotkeeper");
  assert.equal(projectOfCwd("/r/other"), "/r/other");
  assert.equal(sessionProjectName(at("a", "idle", "/r/plotkeeper/.claude/worktrees/x")), "plotkeeper");
  assert.equal(sessionProjectName(at("a", "idle", "/")), "~");
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
});
