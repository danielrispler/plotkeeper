// Ratifying's pure part (ticket #343), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { openRows, overturnPrompt, overturnRoute, ratifyStep, sheetPending, statusWords } from "../../dist/ratify.js";

const session = (id, pr, state = "working", terminal = `t-${id}`) => ({ id, state, terminal, identity: { pr } });
const row = (kind, id, status = "not-yet", extra = {}) => ({ pr: 7, kind, id, text: `${id} text`, change: "", status, ...extra });

test("the sheet marks a law and a link at the part that are not yet ratified, with the PR that changed them", () => {
  const links = [{ id: "a-b", from: "a", to: "b" }, { id: "c-d", from: "c", to: "d" }, { id: "x-a", from: "x", to: "a" }];
  const pending = [{ pr: 9, kind: "law", id: "l1" }, { pr: 8, kind: "link", id: "a-b" }, { pr: 8, kind: "link", id: "c-d" }, { pr: 5, kind: "part", id: "a" }];
  const found = sheetPending(pending, "a", links);
  assert.deepEqual([...found.laws], [["l1", 9]]);
  assert.deepEqual(found.links.map((l) => [l.id, l.pr]), [["a-b", 8]], "only links at this part, only the unratified");
  assert.equal(sheetPending([], "a", links).laws.size, 0);
});

test("an overturn goes to the PR's live session with a terminal, else to New session", () => {
  const sessions = [session("s1", 3), session("s2", 7, "ended"), session("s3", 7, "working", null), session("s4", 7, "needs_you", "t-4")];
  assert.deepEqual(overturnRoute(sessions, 7), { kind: "live", id: "s4", terminal: "t-4" });
  assert.deepEqual(overturnRoute(sessions, 8), { kind: "new" });
  assert.deepEqual(overturnRoute([session("s2", 7, "ended")], 7), { kind: "new" }, "an ended session is not live");
});

test("the overturn prompt quotes the item and its PR", () => {
  const law = overturnPrompt(row("law", "l1", "not-yet", { text: "The hook never waits.", change: "changed" }));
  assert.match(law, /law l1 \(changed\): “The hook never waits\.”/);
  assert.match(law, /PR #7/);
  assert.match(overturnPrompt(row("decision", "d1", "not-yet", { text: "Briefs are kept per project." })), /decision “Briefs are kept per project\.” from PR #7/);
});

test("the ratify step exists only when there is something to ratify, and rows say where they stand", () => {
  assert.equal(ratifyStep([]), null);
  const step = ratifyStep([row("law", "l1")]);
  assert.equal(step.k, "Ratify");
  assert.deepEqual(ratifyStep([row("law", "l1")], ["shell"]).on, ["shell"], "it keeps the parts the brief lit");
  assert.deepEqual(openRows([row("law", "l1"), row("law", "l2", "accepted"), row("decision", "d1", "ratified")]).map((r) => r.id), ["l1"]);
  assert.equal(statusWords["not-yet"], "Not yet ratified");
});
