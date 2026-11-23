// One selection everywhere (danielrispler/Falinks#567): selecting a session over the map, which is the page's base (#386). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";

// The map is the page's base (#386): selecting a session shows its project's map, with its story, or a worker's log, in the left panel.
test("selecting a session shows its project's map and opens it in the panel", async () => {
  const { selectedView } = await import("../../dist/map-model.js");
  assert.deepEqual(selectedView("s2", "/w/falinks", null), { project: "/w/falinks", panel: { kind: "story", session: "s2" } });
  assert.deepEqual(selectedView("s1", "/w/plotkeeper", "w1"), { project: "/w/plotkeeper", panel: { kind: "log", session: "s1", agent: "w1" } });
});

test("a project header's click shows its map; on the shown project's header it folds the group (#386)", async () => {
  const { pressHeader } = await import("../../dist/map-model.js");
  const projectOf = (id) => ({ s1: "/w/plotkeeper", s2: "/w/falinks" })[id] ?? null;
  const bare = { project: "/w/plotkeeper", panel: null };
  assert.deepEqual(pressHeader(bare, "/w/falinks", projectOf), { project: "/w/falinks", panel: null });
  assert.equal(pressHeader(bare, "/w/plotkeeper", projectOf), "fold");
  // A story of another project closes with the switch; History and Settings, which belong to no project, stay.
  assert.deepEqual(pressHeader({ project: "/w/plotkeeper", panel: { kind: "story", session: "s1" } }, "/w/falinks", projectOf), { project: "/w/falinks", panel: null });
  assert.deepEqual(pressHeader({ project: "/w/plotkeeper", panel: { kind: "history" } }, "/w/falinks", projectOf), { project: "/w/falinks", panel: { kind: "history" } });
  // A story of the project switched to stays open.
  assert.deepEqual(pressHeader({ project: "/w/plotkeeper", panel: { kind: "story", session: "s2" } }, "/w/falinks", projectOf), { project: "/w/falinks", panel: { kind: "story", session: "s2" } });
  // A fresh page has no project shown yet: the header shows its map.
  assert.deepEqual(pressHeader({ project: null, panel: null }, "/w/falinks", projectOf), { project: "/w/falinks", panel: null });
});
