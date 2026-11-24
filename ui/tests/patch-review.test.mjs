// Back a law (spec #378, ticket #398), run on the `tsc` output: a law on trust in the part's sheet offers Back a law, a run's patch shows
// as being proven, rejected with why, or proven with Review, and the patch review shows the test patch and both outputs, with Discard.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument } from "./dom.mjs";

globalThis.document = fakeDocument();
const { lawAction, lawActionNodes, reviewNodes } = await import("../../dist/patch-review.js");

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

test("the review shows the law, the new test, the test patch and both outputs, and Discard deletes the patch", () => {
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
  button(nodes, "Discard").listeners.click();
  button(nodes, "Close").listeners.click();
  assert.deepEqual(calls, [["discard", "r3"], ["close"]]);
  assert.equal(button(nodes, "Open draft PR"), undefined, "Open draft PR is the next ticket's");
});
