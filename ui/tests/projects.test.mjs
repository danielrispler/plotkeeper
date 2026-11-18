// The project list's reading and the per-project state kept for it (#98). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";

let calls = 0;
let fail = true;
globalThis.window = {
  __TAURI__: { core: { invoke: async () => (++calls, fail ? Promise.reject("boom") : ["/a"]) } },
};
const { projects } = await import("../../dist/projects.js");
// Loaded by the tests that need it, after the retry above: its listeners draw the sidebar, which needs a DOM.
const workspaces = () => import("../../dist/workspaces.js");

test("a failed read of the projects is tried again by the next call, a good one is kept", async () => {
  await assert.rejects(projects(), /boom/);
  fail = false;
  assert.deepEqual(await projects(), ["/a"]);
  await projects();
  assert.equal(calls, 2);
});

test("state kept per project is dropped for a project no longer listed", async () => {
  const { keepProjects } = await workspaces();
  const states = new Map([["/a", 1], ["/b", 2]]);
  keepProjects(["/b", "/c"], states);
  assert.deepEqual([...states.keys()], ["/b"]);
});

test("with no selection the Git line shows the first project in project order that has been read", async () => {
  const { firstProject } = await workspaces();
  const found = new Map([["/b", "b"], ["/a", "a"]]);
  assert.equal(firstProject(["/a", "/b"], found), "/a");
  assert.equal(firstProject(["/c", "/b"], found), "/b");
  assert.equal(firstProject(["/c"], found), undefined);
});

test("a worktree resolves to its own repository, never the first project, and to none when git finds none", async () => {
  const { repositoryOfWorktree } = await workspaces();
  const worktree = "/other/.claude/worktrees/x";
  assert.equal(await repositoryOfWorktree(worktree, async (folder) => (folder === worktree ? "/other" : "/a")), "/other");
  assert.equal(await repositoryOfWorktree(worktree, async () => null), null);
  assert.equal(await repositoryOfWorktree(worktree, async () => Promise.reject("boom")), null);
});

test("Home's project rows appear once a retried projects read succeeds, with no read error left; a later read does not redraw", async () => {
  const mod = await import("../../dist/projects.js?retry");
  const { homeProjects } = await import("../../dist/home.js");
  // What a listener (Home, the sidebar) sees when it is told.
  const seen = [];
  mod.onProjects(() => seen.push({ rows: homeProjects(mod.knownProjects()), error: mod.projectsError() }));
  fail = true;
  await assert.rejects(mod.projects(), /boom/);
  assert.equal(mod.projectsError(), "boom");
  assert.equal(seen.length, 0);
  fail = false;
  assert.deepEqual(await mod.projects(), ["/a"]);
  assert.deepEqual(seen, [{ rows: [{ name: "a", path: "/a" }], error: null }]);
  await mod.projects();
  assert.equal(seen.length, 1);
});

test("the picker opens on the selected session's project, else the last one started in, else the first", async () => {
  const { pickerProject } = await import("../../dist/projects.js");
  const list = ["/plotkeeper", "/falinks", "/notes"];
  assert.equal(pickerProject("/notes", "/falinks", list), "/notes");
  assert.equal(pickerProject(null, "/falinks", list), "/falinks");
  // A last project since removed is skipped, as is none kept yet.
  assert.equal(pickerProject(null, "/gone", list), "/plotkeeper");
  assert.equal(pickerProject(null, null, list), "/plotkeeper");
  assert.equal(pickerProject(null, null, []), "");
});
