// The projects the page groups its sessions by (#97, #326): a repository some session ran in, a task's worktree in its main
// checkout's project. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { knownProjects, projectGroups, projectOfCwd, repositoryOf, selectedProject, sessionProjectName, sidebarOrder } from "../../dist/projects.js";
import { studio } from "../../dist/state.js";

const at = (id, state, cwd) => ({ id, state, cwd });

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

test("the Map view's projects are every repository a listed session ran in, ended ones too, and the selected session's is its own", () => {
  studio.listed = [at("a", "idle", "/r/p/.claude/worktrees/x"), at("b", "ended", "/r/q")];
  assert.deepEqual(knownProjects(), ["/r/p", "/r/q"]);
  studio.selection = null;
  assert.equal(selectedProject(), null);
  studio.selection = { session: studio.listed[0], agent: null };
  assert.equal(selectedProject(), "/r/p");
});
