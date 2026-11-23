// Pre-flight (spec #378, ticket #394), run on the `tsc` output: its inputs, the hand picker after a failed prediction, the predicted parts
// with their laws on trust, fog and staleness, the brief's expected-against-touched line, and no single-key shortcut while typing in its field.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument } from "./dom.mjs";

globalThis.document = fakeDocument();
const { bands, expectedLine, preFlightNodes, preFlightRows, settle } = await import("../../dist/map-pre-flight.js");
const { windowBinding } = await import("../../dist/keys.js");
const { initial } = await import("../../dist/settings.js");

const part = (id, name) => ({ id, name, caption: "", description: "", tier: "core", paths: [] });
const doc = {
  ok: true,
  parts: [part("hook", "Hook"), part("core", "Core"), part("window", "Window")],
  actors: [],
  links: [],
  laws: [
    { id: "core-order", on: "core", backing: "trust", text: "Events apply in order." },
    { id: "core-pure", on: "core", backing: "check", text: "The core reads no file." },
    { id: "hook-held", on: "hook", backing: "trust", text: "A held question blocks the agent." },
  ],
};
const state = { fog: { core: "thick" } };
const pre = (extra = {}) => ({ issue: 378, title: "Studio: Spec: the modules", parts: ["core", "hook"], predicted: true, at: 1, ...extra });
const acts = (calls) => ({
  predict: (issue, text) => calls.push(["predict", issue, text]),
  byHand: () => calls.push(["byHand"]),
  pick: (parts) => calls.push(["pick", parts]),
  clear: () => calls.push(["clear"]),
});
const buttons = (nodes) => nodes.flatMap((node) => node.find("button"));
const button = (nodes, words) => buttons(nodes).find((each) => each.text.includes(words));
const texts = (nodes) => nodes.map((node) => node.text).join("\n");

test("the predicted parts show with their laws on trust, their fog and their staleness, in the pre-flight's order", () => {
  bands.of = (_, id) => (id === "hook" ? "not looked at in 6 weeks" : null);
  try {
    assert.deepEqual(preFlightRows(pre({ parts: ["core", "hook", "gone"] }), doc, state), [
      { id: "core", name: "Core", trust: ["Events apply in order."], fog: "fog: laws, links or flows changed", stale: null },
      { id: "hook", name: "Hook", trust: ["A held question blocks the agent."], fog: null, stale: "not looked at in 6 weeks" },
    ]);
  } finally {
    bands.of = () => null;
  }
});

test("a failed prediction shows the hand picker with why; a prediction frames its parts", async () => {
  assert.deepEqual(await settle(Promise.reject("the writer named no part on the map")), { kind: "pick", why: "the writer named no part on the map" });
  assert.deepEqual(await settle(Promise.resolve(pre())), { kind: "result", pre: pre() });
});

test("the inputs: an open issue, one or two lines of text, or parts by hand", () => {
  const calls = [];
  const nodes = preFlightNodes({ kind: "start", issues: [{ number: 394, title: "Studio: pre-flight" }] }, doc, state, acts(calls));
  button(nodes, "#394").listeners.click();
  const [field] = nodes.flatMap((node) => node.find("textarea"));
  assert.ok(field, "the free text is a text field");
  field.value = "  Make the hook faster  ";
  button(nodes, "Predict").listeners.click();
  button(nodes, "Pick parts by hand").listeners.click();
  assert.deepEqual(calls, [["predict", 394, ""], ["predict", null, "Make the hook faster"], ["byHand"]]);
  assert.match(texts(preFlightNodes({ kind: "start", issues: null }, doc, state, acts([]))), /Reading the open issues/);
});

test("no single-key shortcut fires while typing in the pre-flight's field, and Esc still closes what is on top", () => {
  const key = (code, k) => ({ key: k, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false });
  assert.equal(windowBinding(initial.keymap, key("Digit1", "1"), false, false, true)?.[0], "studio::Answer", "1 is a shortcut outside a field");
  assert.equal(windowBinding(initial.keymap, key("Digit1", "1"), true, false, true), undefined);
  assert.equal(windowBinding(initial.keymap, key("Escape", "Escape"), true), "studio::Dismiss");
});

test("the hand picker lists every part and frames the ones ticked", () => {
  const calls = [];
  const nodes = preFlightNodes({ kind: "pick", why: "the writer named no part on the map" }, doc, state, acts(calls));
  assert.match(texts(nodes), /the writer named no part on the map/);
  const boxes = nodes.flatMap((node) => node.find("input"));
  assert.deepEqual(boxes.map((box) => box.value), ["hook", "core", "window"]);
  boxes[0].checked = true;
  boxes[2].checked = true;
  button(nodes, "Show these parts").listeners.click();
  assert.deepEqual(calls, [["pick", ["hook", "window"]]]);
});

test("a pre-flight framed on the map lists its parts as predicted, and Clear takes it away", () => {
  const calls = [];
  const nodes = preFlightNodes({ kind: "result", pre: pre() }, doc, state, acts(calls));
  const shown = texts(nodes);
  assert.match(shown, /#378 Studio: Spec: the modules/);
  assert.match(shown, /Predicted/);
  assert.match(shown, /Core.*Events apply in order\./s);
  assert.doesNotMatch(shown, /The core reads no file/, "a law backed by a check is not a blind spot");
  button(nodes, "Clear").listeners.click();
  assert.deepEqual(calls, [["clear"]]);
  assert.match(texts(preFlightNodes({ kind: "result", pre: pre({ issue: null, title: "", predicted: false }) }, doc, state, acts([]))), /Picked by hand/);
});

test("a matched brief says what was expected against what it touched; an unmatched one says nothing", () => {
  assert.equal(expectedLine({ expected: ["hook", "core"], touched: ["window", "hook"] }, doc), "You expected Hook, Core; it touched Window, Hook");
  assert.equal(expectedLine(undefined, doc), null);
  assert.equal(expectedLine({ expected: ["gone"], touched: [] }, null), "You expected gone; it touched nothing");
});
