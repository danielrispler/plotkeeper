// Back a law (spec #378, ticket #398), run on the `tsc` output: a law on trust in the part's sheet offers Back a law, a run's patch shows
// as being proven, rejected with why, or proven with Review, and the patch review shows the test patch and both outputs, with Open draft PR
// and Discard (#400). An opened patch says its PR; a stale one says "Out of date, run again".
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument, tauri } from "./dom.mjs";

globalThis.document = fakeDocument();
const { confirmNodes, guardNodes, guardOffer, guardRow, lawAction, lawActionNodes, openedLine, reviewNodes } = await import("../../dist/patch-review.js");

const patch = (id, state, extra = {}) => ({ id, project: "/w/app", law: "core-order", state, line: null, at: 1, diff: null, proof: null, ...extra });
const proven = (id = "r3") =>
  patch(id, "proven", {
    at: 3,
    diff: "diff --git a/core/tests/order.rs b/core/tests/order.rs\n--- /dev/null\n+++ b/core/tests/order.rs\n@@ -0,0 +1,2 @@\n+// @law: core-order\n+fn events_apply_in_order() {}\n",
    proof: { test: "events_apply_in_order", file: "core/tests/order.rs", passed: "PASS [ 0.01s] studio-core::it order::events_apply_in_order", failed: "FAIL [ 0.01s] studio-core::it order::events_apply_in_order" },
  });
// `confirm` answers a confirm dialog with `yes`, and notes what it asked.
const acts = (calls, yes = true) => ({
  start: (law) => calls.push(["start", law]),
  review: (id) => calls.push(["review", id]),
  open: (id) => calls.push(["open", id]),
  discard: (id) => calls.push(["discard", id]),
  close: () => calls.push(["close"]),
  confirm: async (ask) => (calls.push(["confirm", ask.verb]), yes),
});
// Lets the confirm's answer land.
const settle = () => new Promise((done) => setTimeout(done));
const button = (nodes, words) => nodes.flatMap((node) => node.find("button")).find((each) => each.text === words);
const texts = (nodes) => nodes.map((node) => node.text).join("\n");

test("a law with no patch offers Back a law, and the click starts a run for that law", () => {
  assert.deepEqual(lawAction("core-order", []), { law: "core-order", kind: "start" });
  assert.deepEqual(lawAction("core-order", undefined), { law: "core-order", kind: "start" }, "a payload from before back a law has none");
  const calls = [];
  const nodes = lawActionNodes(lawAction("core-order", [patch("x", "proven", { law: "other" })]), acts(calls));
  button(nodes, "Back a law").listeners.click();
  assert.deepEqual(calls, [["start", "core-order"]]);
});

test("the law's newest patch decides: being proven, rejected with why and Dismiss, or proven with Review", () => {
  assert.deepEqual(lawAction("core-order", [patch("r1", "proving")]), { law: "core-order", kind: "proving" });
  const rejected = patch("r2", "rejected", { at: 2, line: "The test patch touches src/a.rs, outside a tests folder and the map\nmore" });
  assert.deepEqual(lawAction("core-order", [patch("r1", "proving"), rejected]), { law: "core-order", kind: "rejected", id: "r2", line: "The test patch touches src/a.rs, outside a tests folder and the map" });
  assert.deepEqual(lawAction("core-order", [rejected, proven()]), { law: "core-order", kind: "proven", id: "r3" });

  const calls = [];
  assert.match(texts(lawActionNodes({ law: "core-order", kind: "proving" }, acts(calls))), /Proving the patch/);
  assert.equal(button(lawActionNodes({ law: "core-order", kind: "proving" }, acts(calls)), "Back a law"), undefined, "no second run while one is proven");
  const no = lawActionNodes(lawAction("core-order", [rejected]), acts(calls));
  assert.match(texts(no), /Rejected: The test patch touches src\/a.rs/);
  button(no, "Dismiss").listeners.click();
  button(no, "Back a law").listeners.click();
  button(lawActionNodes(lawAction("core-order", [proven()]), acts(calls)), "Review the patch").listeners.click();
  assert.deepEqual(calls, [["discard", "r2"], ["start", "core-order"], ["review", "r3"]]);
});

test("the review shows the law, the new test, the test patch and both outputs, with Open draft PR and Discard", async () => {
  const calls = [];
  const nodes = reviewNodes(proven(), "Events apply in order.", acts(calls));
  const all = texts(nodes);
  assert.match(all, /Events apply in order\./);
  assert.match(all, /events_apply_in_order in core\/tests\/order\.rs/);
  const lines = nodes.flatMap((node) => node.find("pre")).filter((pre) => pre.className.startsWith("diff-line"));
  assert.deepEqual(
    lines.map((line) => line.className),
    ["diff-line meta", "diff-line meta", "diff-line meta", "diff-line hunk", "diff-line add", "diff-line add"],
  );
  assert.match(all, /With the test alone, the checks pass/);
  assert.match(all, /PASS \[ 0\.01s\] studio-core::it order::events_apply_in_order/);
  assert.match(all, /With the plant, the new test fails/);
  assert.match(all, /FAIL \[ 0\.01s\] studio-core::it order::events_apply_in_order/);
  button(nodes, "Open draft PR").listeners.click();
  button(nodes, "Close").listeners.click();
  button(nodes, "Discard").listeners.click();
  await settle();
  assert.deepEqual(calls, [["open", "r3"], ["close"], ["confirm", "Discard patch"], ["discard", "r3"]], "Discard asks first");
});

test("Discard declined sends nothing; Dismiss on a rejected run acts at once", async () => {
  const calls = [];
  button(reviewNodes(proven(), "Events apply in order.", acts(calls, false)), "Discard").listeners.click();
  button(lawActionNodes(lawAction("core-order", [patch("r5", "out-of-date", { at: 5 })]), acts(calls, false)), "Discard").listeners.click();
  await settle();
  assert.deepEqual(calls, [["confirm", "Discard patch"], ["confirm", "Discard patch"]]);
  button(lawActionNodes(lawAction("core-order", [patch("r2", "rejected", { line: "no" })]), acts(calls, false)), "Dismiss").listeners.click();
  assert.deepEqual(calls.at(-1), ["discard", "r2"]);
});

test("an opened patch says its draft PR, and offers no second run", () => {
  const opened = patch("r4", "opened", { at: 4, pr: 412, url: "https://github.com/someone/app/pull/412" });
  assert.deepEqual(lawAction("core-order", [proven(), opened]), { law: "core-order", kind: "opened", id: "r4", pr: 412, url: "https://github.com/someone/app/pull/412" });
  const calls = [];
  const nodes = lawActionNodes(lawAction("core-order", [opened]), acts(calls));
  assert.match(texts(nodes), /Draft PR #412 opened/);
  assert.equal(button(nodes, "Back a law"), undefined);
  const link = nodes.flatMap((node) => [node, ...node.find("a")]).find((node) => node.tag === "a");
  assert.equal(link?.href, "https://github.com/someone/app/pull/412");
});

test("a stale patch says Out of date, run again, with Discard and Back a law", async () => {
  const stale = patch("r5", "out-of-date", { at: 5 });
  assert.deepEqual(lawAction("core-order", [proven(), stale]), { law: "core-order", kind: "out-of-date", id: "r5" });
  const calls = [];
  const nodes = lawActionNodes(lawAction("core-order", [stale]), acts(calls));
  assert.match(texts(nodes), /Out of date, run again/);
  button(nodes, "Back a law").listeners.click();
  button(nodes, "Discard").listeners.click();
  await settle();
  assert.deepEqual(calls, [["start", "core-order"], ["confirm", "Discard patch"], ["discard", "r5"]]);
});

test("the outcome of Open draft PR is said in one line", () => {
  assert.equal(openedLine({ kind: "pr", number: 412, url: "https://github.com/someone/app/pull/412" }), "Draft PR #412 opened: https://github.com/someone/app/pull/412");
  assert.equal(openedLine({ kind: "out-of-date" }), "Out of date, run again: the patch no longer applies to the default branch");
});

// ---- Guard with a law (#401): a row of a bug fix's brief, beside Ratify.

const laws = [
  { id: "core-order", on: "core", backing: "trust", text: "Events apply in order." },
  { id: "core-backed", on: "core", backing: "check", text: "Backed already." },
  { id: "shell-only", on: "shell", backing: "trust", text: "Not touched." },
];
const guardActs = (calls) => ({
  ...acts(calls),
  back: (law) => calls.push(["back", law]),
  add: () => calls.push(["add"]),
});

test("Guard with a law offers backing an on-trust law of a part the PR touched, or adding a new law", () => {
  const offer = guardOffer({ parts: ["core", "hook"] }, 7, laws, [patch("n1", "proven", { law: "core-new", text: "A new one.", guards: 7 }), patch("n2", "proven", { law: "other-new", text: "Another PR's.", guards: 8 })]);
  assert.deepEqual(offer.back.map((law) => law.id), ["core-order"], "on trust and on a touched part only");
  assert.deepEqual(offer.added.map((p) => p.id), ["n1"], "the new laws this PR's guard started");
  const calls = [];
  const row = guardNodes(offer, [], guardActs(calls));
  assert.match(row.text, /Guard with a law/);
  assert.match(row.text, /Events apply in order\./);
  assert.match(row.text, /A new one\./);
  button([row], "Back this law").listeners.click();
  button([row], "Add a new law").listeners.click();
  button([row], "Review the patch").listeners.click();
  assert.deepEqual(calls, [["back", "core-order"], ["add"], ["review", "n1"]]);
});

test("a new law's rejected run offers Dismiss, not a back a law run of a law the map does not have", () => {
  const offer = guardOffer({ parts: ["core"] }, 7, [], [patch("n1", "rejected", { law: "core-new", text: "A new one.", guards: 7, line: "no" })]);
  const row = guardNodes(offer, offer.added, guardActs([]));
  assert.ok(button([row], "Dismiss"));
  assert.equal(button([row], "Back a law"), undefined);
});

test("the drafted sentence is shown for confirmation, and the edited one is what starts the run", () => {
  const calls = [];
  const draft = { id: "core-keeps-order", on: "core", text: "Events never reorder." };
  const nodes = confirmNodes(draft, [{ id: "core", name: "Core" }, { id: "hook", name: "Hook" }], null, {
    start: (law) => calls.push(["start", law]),
    cancel: () => calls.push(["cancel"]),
  });
  const fields = nodes.flatMap((node) => [...node.find("input"), ...node.find("textarea"), ...node.find("select")]);
  const sentence = fields.find((field) => field.tag === "textarea");
  assert.equal(sentence.value, "Events never reorder.", "the writer's draft, to confirm or edit");
  assert.equal(fields.find((field) => field.tag === "input").value, "core-keeps-order");
  sentence.value = "Events apply in the order they were made.";
  button(nodes, "Start the run").listeners.click();
  button(nodes, "Cancel").listeners.click();
  assert.deepEqual(calls, [["start", { id: "core-keeps-order", on: "core", text: "Events apply in the order they were made." }], ["cancel"]]);
});

test("what Daniel typed in the confirm step stays when the map draws the brief again", () => {
  const draft = { id: "core-keeps-order", on: "core", text: "Events never reorder." };
  const parts = [{ id: "core", name: "Core" }, { id: "hook", name: "Hook" }];
  const act = { start: () => {}, cancel: () => {} };
  const fields = (nodes) => nodes.flatMap((node) => [...node.find("input"), ...node.find("textarea"), ...node.find("select")]);
  const [part, id, sentence] = ["select", "input", "textarea"].map((tag) => fields(confirmNodes(draft, parts, null, act)).find((f) => f.tag === tag));
  // Daniel types into each field; then a redraw (a map-landed, a resize) builds the step again from the same draft.
  part.value = "hook";
  part.listeners.change();
  id.value = "hook-keeps-order";
  id.listeners.input();
  sentence.value = "Hooks fire in order.";
  sentence.listeners.input();
  const again = fields(confirmNodes(draft, parts, null, act));
  assert.deepEqual(
    ["select", "input", "textarea"].map((tag) => again.find((f) => f.tag === tag).value),
    ["hook", "hook-keeps-order", "Hooks fire in order."],
  );
});

test("while the writer drafts, the confirm step says so; when it cannot, Daniel writes the law himself", () => {
  const act = { start: () => {}, cancel: () => {} };
  const drafting = confirmNodes(null, [{ id: "core", name: "Core" }], null, act);
  assert.match(texts(drafting), /Drafting the law/);
  assert.equal(button(drafting, "Start the run"), undefined);
  const failed = confirmNodes({ id: "", on: "core", text: "" }, [{ id: "core", name: "Core" }], "the writer's reply holds no JSON", act);
  assert.match(texts(failed), /the writer's reply holds no JSON/);
  assert.ok(button(failed, "Start the run"));
});

test("a run that cannot start, whose prompt the clipboard refuses, shows the prompt to copy by hand", async () => {
  const box = new El("section");
  globalThis.document = fakeDocument({ getElementById: (id) => (id === "map-patch-review" ? box : new El("div")) });
  tauri([], (command) => (command === "back_a_law" ? { kind: "refused", line: "srt is not installed", prompt: "Write a test for core-order…" } : null));
  const view = {
    project: "/w/app",
    scrub: null,
    doc: { parts: [{ id: "core", name: "Core" }], laws: [{ id: "core-order", on: "core", backing: "trust", text: "Events apply in order." }] },
    state: { law_patches: [] },
  };
  const row = guardRow(view, 7, { parts: ["core"] }, () => {});
  // No clipboard here, as when a browser refuses it after the round trip.
  button([row], "Back this law").listeners.click();
  await new Promise((done) => setTimeout(done, 10));
  assert.equal(box.hidden, false);
  assert.match(box.text, /srt is not installed/);
  assert.deepEqual(box.find("textarea").map((area) => area.value), ["Write a test for core-order…"]);
  globalThis.document = fakeDocument();
});
