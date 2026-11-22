// The per-turn Quiz, Retests, Home's learning slot and the trial are retired (spec #334, ticket #347). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fakeDocument, tauri } from "./dom.mjs";

tauri([], () => null);
// The document is set after the import: times.ts starts a timer when one exists at load.
const home = await import("../../dist/home.js");
const { initial } = await import("../../dist/settings.js");
globalThis.document = fakeDocument({});

test("the Quiz's window code is gone", () => {
  assert.equal(fs.existsSync(new URL("../quiz.ts", import.meta.url)), false);
});

test("Home has no learning slot, no Retest and no 'shown' watch", () => {
  for (const name of ["slotNode", "slotLabel", "learningSlot", "watchLearningShown", "shownDue", "endTracker", "turnEnded"]) {
    assert.equal(name in home, false, name);
  }
  const state = { expanded: new Set(), cleared: new Set() };
  const sessions = [{ id: "u", state: "your_turn", unseen: true, landed: false, turns: [], questions: [], transcript: {} }];
  assert.deepEqual(home.homeView(sessions, ["/w/a"], state).map((p) => p.key), ["unseen", "start"]);
  assert.equal("quiet" in home.homeState || "held" in home.homeState, false);
});

test("the settings hold no Quiz trial", () => {
  assert.equal("quiz_trial" in initial.settings, false);
});
