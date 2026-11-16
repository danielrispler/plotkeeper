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

test("a read that succeeds after a failure tells the listeners, as an add does; a later read does not", async () => {
  const mod = await import("../../dist/projects.js?retry");
  let seen = 0;
  mod.onProjects(() => seen++);
  fail = true;
  await assert.rejects(mod.projects(), /boom/);
  assert.equal(seen, 0);
  fail = false;
  assert.deepEqual(await mod.projects(), ["/a"]);
  assert.deepEqual(mod.knownProjects(), ["/a"]);
  assert.equal(seen, 1);
  await mod.projects();
  assert.equal(seen, 1);
});
