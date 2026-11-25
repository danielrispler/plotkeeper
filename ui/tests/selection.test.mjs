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

// #548: an event that came while a press waited is drawn by a read after the press settles. When that read fails, the event is still
// missed, so the next press reads again; and the failure is logged, not left as an unhandled rejection of the press.
test("a catch-up whose read fails keeps the missed event for the next press", async (t) => {
  const { fakeDocument, tauri } = await import("./dom.mjs");
  // The page's modules tick the shown times on an interval, which would keep this test's process alive.
  t.mock.timers.enable({ apis: ["setInterval"] });
  globalThis.document = fakeDocument();
  const calls = [];
  let down = true;
  let studio;
  tauri(calls, (command) => {
    // An event arrives while the press waits for the server.
    if (command === "select") studio.missed = true;
    if (command === "selected" && down) throw new Error("the server is gone");
    if (command === "cards") return { brief: null, goal: null, answers: [], open: [], design: [], decisions: [], risk_notes: [], decision_notes: [] };
    return null;
  });
  const logged = [];
  // Only `warn` is stood in for: an `error` line would fail to log, as no line here should be one.
  window.__TAURI__.log = /** @type {any} */ ({ warn: async (line) => void logged.push(line) });
  ({ studio } = await import("../../dist/state.js"));
  const { select } = await import("../../dist/selection.js");
  await select("s1");
  assert.equal(studio.missed, true, "the failed read left the event missed");
  assert.deepEqual(logged, ["catch up: the server is gone"]);
  // The server is back: the next press's catch-up reads, and nothing is missed any more.
  down = false;
  calls.length = 0;
  await select("s1");
  assert.equal(studio.missed, false);
  assert.deepEqual(calls.map(([command]) => command), ["select", "cards", "selected", "cards"]);
});
