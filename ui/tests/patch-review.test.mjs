// Back a law (spec #378, ticket #398), run on the `tsc` output: a law on trust in the part's sheet offers Back a law, a run's patch shows
// as being proven, rejected with why, or proven with Review, and the patch review shows the test patch and both outputs, with Open draft PR
// and Discard (#400). An opened patch says its PR; a stale one says "Out of date, run again".
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument } from "./dom.mjs";

globalThis.document = fakeDocument();
const { lawAction, lawActionNodes, openedLine, reviewNodes } = await import("../../dist/patch-review.js");

const patch = (id, state, extra = {}) => ({ id, project: "/w/app", law: "core-order", state, line: null, at: 1, diff: null, proof: null, ...extra });
const proven = (id = "r3") =>
  patch(id, "proven", {
    at: 3,
    diff: "diff --git a/core/tests/order.rs b/core/tests/order.rs\n--- /dev/null\n+++ b/core/tests/order.rs\n@@ -0,0 +1,2 @@\n+// @law: core-order\n+fn events_apply_in_order() {}\n",
    proof: { test: "events_apply_in_order", file: "core/tests/order.rs", passed: "PASS [ 0.01s] studio-core::it order::events_apply_in_order", failed: "FAIL [ 0.01s] studio-core::it order::events_apply_in_order" },
  });
const acts = (calls) => ({
  start: (law) => calls.push(["start", law]),
  review: (id) => calls.push(["review", id]),
  open: (id) => calls.push(["open", id]),
  discard: (id) => calls.push(["discard", id]),
  close: () => calls.push(["close"]),
});
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

test("the review shows the law, the new test, the test patch and both outputs, with Open draft PR and Discard", () => {
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
  button(nodes, "Discard").listeners.click();
  button(nodes, "Close").listeners.click();
  assert.deepEqual(calls, [["open", "r3"], ["discard", "r3"], ["close"]]);
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

test("a stale patch says Out of date, run again, with Discard and Back a law", () => {
  const stale = patch("r5", "out-of-date", { at: 5 });
  assert.deepEqual(lawAction("core-order", [proven(), stale]), { law: "core-order", kind: "out-of-date", id: "r5" });
  const calls = [];
  const nodes = lawActionNodes(lawAction("core-order", [stale]), acts(calls));
  assert.match(texts(nodes), /Out of date, run again/);
  button(nodes, "Discard").listeners.click();
  button(nodes, "Back a law").listeners.click();
  assert.deepEqual(calls, [["discard", "r5"], ["start", "core-order"]]);
});

test("the outcome of Open draft PR is said in one line", () => {
  assert.equal(openedLine({ kind: "pr", number: 412, url: "https://github.com/someone/app/pull/412" }), "Draft PR #412 opened: https://github.com/someone/app/pull/412");
  assert.equal(openedLine({ kind: "out-of-date" }), "Out of date, run again: the patch no longer applies to the default branch");
});
