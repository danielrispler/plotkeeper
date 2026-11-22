// The rail's "PR brief ready" row (spec danielrispler/plotkeeper#334, ticket #344), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument } from "./dom.mjs";

globalThis.document = fakeDocument();
const { briefRowFor } = await import("../../dist/rail-brief.js");

const session = (pr, state = "idle", extra = {}) => ({ id: "s", cwd: "/w/p", state, terminal: 3, identity: { issue: null, branch: "b", pr, worktree: "/w/p", name: "n" }, ...extra });
const waiting = { project: "/p", numbers: [7] };

test("the row is there for a live session whose PR has a brief waiting, and says so quietly", () => {
  const row = briefRowFor(session(7), waiting, () => {});
  assert.ok(row);
  assert.equal(row.textContent, "PR brief ready");
});

test("a PR with no live studio session has no row", () => {
  assert.equal(briefRowFor(session(7, "ended"), waiting, () => {}), null);
  assert.equal(briefRowFor(session(null), waiting, () => {}), null);
});

test("no row while no brief waits for the session's PR", () => {
  assert.equal(briefRowFor(session(8), waiting, () => {}), null);
  assert.equal(briefRowFor(session(7), null, () => {}), null);
});

test("pressing the row opens the Map view on that brief", () => {
  const opened = [];
  const row = briefRowFor(session(7), waiting, (project, number) => opened.push([project, number]));
  row.listeners.click();
  assert.deepEqual(opened, [["/p", 7]]);
});
