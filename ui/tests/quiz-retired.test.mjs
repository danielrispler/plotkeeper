// The per-turn Quiz, Retests, Home's learning slot and the trial are retired (spec #334, ticket #347). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fakeDocument, tauri } from "./dom.mjs";

tauri([], () => null);
// The document is set after the import: times.ts starts a timer when one exists at load.
const { initial } = await import("../../dist/settings.js");
globalThis.document = fakeDocument({});

test("the Quiz's window code is gone", () => {
  assert.equal(fs.existsSync(new URL("../quiz.ts", import.meta.url)), false);
});

test("the settings hold no Quiz trial", () => {
  assert.equal("quiz_trial" in initial.settings, false);
});
