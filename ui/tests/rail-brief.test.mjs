// The rail's "PR brief ready" row (spec danielrispler/plotkeeper#334, ticket #344) and the session row's mark (#630), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument } from "./dom.mjs";

globalThis.document = fakeDocument();
const { briefMarkFor, briefRowFor } = await import("../../dist/rail-brief.js");
const { hintOf } = await import("../../dist/dom.js");

// A session as the core sends it, less the fields the row never reads.
/** @param {number | null} pr @param {import("../../dist/types.js").SessionState} state */
const session = (pr, state = "idle", extra = {}) => /** @type {Pick<import("../../dist/types.js").Session, "identity" | "state">} */ ({ id: "s", cwd: "/w/p", state, identity: { issue: null, branch: "b", pr, worktree: "/w/p", name: "n" }, ...extra });
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
  // The stand-in DOM's node, read as such.
  const row = /** @type {any} */ (briefRowFor(session(7), waiting, (project, number) => opened.push([project, number])));
  row.listeners.click();
  assert.deepEqual(opened, [["/p", 7]]);
});

test("a live session whose PR has a brief waiting shows the row's mark, named by its PR", () => {
  const mark = /** @type {any} */ (briefMarkFor({ ...session(7), id: "s" }, waiting, () => {}));
  assert.ok(mark);
  assert.equal(mark.tag, "button");
  assert.equal(mark.getAttribute("aria-label"), "Open the brief of PR #7");
  assert.equal(hintOf(mark)?.textContent, "PR #7 · brief waiting");
});

test("no mark for an ended session, a session with no PR, or a PR no brief waits for", () => {
  assert.equal(briefMarkFor({ ...session(7, "ended"), id: "s" }, waiting, () => {}), null);
  assert.equal(briefMarkFor({ ...session(null), id: "s" }, waiting, () => {}), null);
  assert.equal(briefMarkFor({ ...session(8), id: "s" }, waiting, () => {}), null);
  assert.equal(briefMarkFor({ ...session(7), id: "s" }, null, () => {}), null);
});

test("pressing the mark opens that brief on the Map view", () => {
  const opened = [];
  const mark = /** @type {any} */ (briefMarkFor({ ...session(7), id: "s" }, waiting, (project, number) => opened.push([project, number])));
  mark.listeners.click();
  assert.deepEqual(opened, [["/p", 7]]);
});
