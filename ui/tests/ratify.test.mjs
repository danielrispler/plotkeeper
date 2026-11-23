// Ratifying's pure part (ticket #343), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { copyOverturn, decideWith, noteFor, openRows, overturnPrompt, overturnReport, ratifyStep, sheetPending, statusWords } from "../../dist/ratify.js";

const row = (kind, id, status = "not-yet", extra = {}) => ({ pr: 7, kind, id, text: `${id} text`, change: "", status, ...extra });

test("the sheet marks a law and a link at the part that are not yet ratified, with the PR that changed them", () => {
  const links = [{ id: "a-b", from: "a", to: "b" }, { id: "c-d", from: "c", to: "d" }, { id: "x-a", from: "x", to: "a" }];
  const pending = [{ pr: 9, kind: "law", id: "l1" }, { pr: 8, kind: "link", id: "a-b" }, { pr: 8, kind: "link", id: "c-d" }, { pr: 5, kind: "part", id: "a" }];
  const found = sheetPending(pending, "a", links);
  assert.deepEqual([...found.laws], [["l1", 9]]);
  assert.deepEqual(found.links.map((l) => [l.id, l.pr]), [["a-b", 8]], "only links at this part, only the unratified");
  assert.equal(sheetPending([], "a", links).laws.size, 0);
});

test("the overturn prompt stands alone: the PR, its branch while it is open, the item's id and its wording", () => {
  const law = row("law", "l1", "not-yet", { text: "The hook never waits.", change: "changed" });
  const open = overturnPrompt(law, "fog-on-the-map");
  assert.match(open, /PR #7/);
  assert.match(open, /branch fog-on-the-map/);
  assert.match(open, /law l1 \(changed\): “The hook never waits\.”/);
  assert.match(open, /Undo it/);
  const merged = overturnPrompt(law, null);
  assert.match(merged, /PR #7/);
  assert.doesNotMatch(merged, /branch/, "a PR that is not open has no branch to name");
  const decision = overturnPrompt(row("decision", "d1", "not-yet", { text: "Briefs are kept per project." }), null);
  assert.match(decision, /decision d1: “Briefs are kept per project\.”/);
  assert.match(decision, /PR #7/);
});

test("copying the prompt says so, and a clipboard that fails or is missing leaves the prompt to copy by hand", async () => {
  const written = [];
  assert.equal(await copyOverturn("the prompt", async (text) => void written.push(text)), "copied");
  assert.deepEqual(written, ["the prompt"]);
  assert.equal(await copyOverturn("the prompt", async () => Promise.reject(new Error("denied"))), "manual");
  assert.equal(await copyOverturn("the prompt", () => { throw new Error("blocked"); }), "manual");
  assert.equal(await copyOverturn("the prompt", undefined), "manual");
  assert.deepEqual(overturnReport("copied", "the prompt"), { line: "Prompt copied, paste it into your agent", manual: null });
  assert.deepEqual(overturnReport("manual", "the prompt"), { line: "Copy this prompt and paste it into your agent", manual: "the prompt" });
});

test("the ratify step exists only when there is something to ratify, and rows say where they stand", () => {
  assert.equal(ratifyStep([]), null);
  const step = ratifyStep([row("law", "l1")]);
  assert.equal(step.k, "Ratify");
  assert.deepEqual(ratifyStep([row("law", "l1")], ["shell"]).on, ["shell"], "it keeps the parts the brief lit");
  assert.deepEqual(openRows([row("law", "l1"), row("law", "l2", "accepted"), row("decision", "d1", "ratified")]).map((r) => r.id), ["l1"]);
  assert.equal(statusWords["not-yet"], "Not yet ratified");
});

test("an overturn's note shows only where its item is drawn: its project, and the item's PR, kind and id", () => {
  const last = { project: "/p", pr: 7, kind: "law", id: "l1", line: "Prompt copied, paste it into your agent", manual: null };
  const brief = (pr, rows) => (item) => rows.some((r) => item.pr === pr && r.kind === item.kind && r.id === item.id);
  assert.equal(noteFor(null, "/p", () => true), null);
  assert.equal(noteFor(last, "/p", brief(7, [{ kind: "law", id: "l1" }])), last);
  assert.equal(noteFor(last, "/q", brief(7, [{ kind: "law", id: "l1" }])), null, "another project");
  assert.equal(noteFor(last, "/p", brief(8, [{ kind: "law", id: "l1" }])), null, "another PR's brief");
  assert.equal(noteFor(last, "/p", brief(7, [{ kind: "link", id: "l1" }])), null, "another item");
  assert.equal(noteFor(last, "/p", () => false), null, "another part's sheet");
});

test("an overturn copies its prompt within the click, before the decision's round trip, and Accept copies nothing", async () => {
  const order = [];
  const copy = async () => (order.push("copy"), "copied");
  const record = async () => {
    order.push("record");
    await new Promise((resolve) => setTimeout(resolve, 1));
  };
  assert.equal(await decideWith(true, copy, record), "copied");
  assert.deepEqual(order, ["copy", "record"], "the clipboard is written before anything is awaited");
  order.length = 0;
  assert.equal(await decideWith(false, copy, record), null);
  assert.deepEqual(order, ["record"]);
});
