// Pre-flight (spec #378, ticket #394), run on the `tsc` output: its inputs, the hand picker after a failed prediction, the predicted parts
// with their laws on trust, fog and staleness, the brief's expected-against-touched line, and no single-key shortcut while typing in its field.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument } from "./dom.mjs";

globalThis.document = fakeDocument();
const { expectedLine, paintPreFlight, preFlightNodes, preFlightRows, settle } = await import("../../dist/map-pre-flight.js");
const { windowBinding } = await import("../../dist/keys.js");
const { initial } = await import("../../dist/settings.js");

const part = (id, name) => ({ id, name, caption: "", purpose: "", holds: [], tier: "core", paths: [] });
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

test("the predicted parts show with their laws on trust, their fog and their staleness band, in the pre-flight's order", () => {
  const stale = { ...state, stale: { hook: 2 } };
  assert.deepEqual(preFlightRows(pre({ parts: ["core", "hook", "gone"] }), doc, stale), [
    { id: "core", name: "Core", trust: ["Events apply in order."], fog: "fog: laws, links or flows changed", stale: null },
    { id: "hook", name: "Hook", trust: ["A held question blocks the agent."], fog: null, stale: "You last engaged with this part 6 weeks or more ago" },
  ]);
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

test("a map redraw keeps what was typed and ticked in the overlay, and its focus, until the overlay moves on", async () => {
  const { El } = await import("./dom.mjs");
  const box = new El("div");
  const start = { kind: "start", issues: [] };
  paintPreFlight(box, start, doc, state, acts([]));
  const [field] = box.find("textarea");
  field.value = "Make the hook faster";
  // The map is drawn again (a landed push, window focus) over a new payload.
  paintPreFlight(box, start, doc, { ...state }, acts([]));
  assert.equal(box.find("textarea")[0], field, "the same field, so its focus stays");
  assert.equal(field.value, "Make the hook faster");

  const pick = { kind: "pick", why: null };
  paintPreFlight(box, pick, doc, state, acts([]));
  const ticked = box.find("input")[1];
  ticked.checked = true;
  paintPreFlight(box, pick, doc, { ...state }, acts([]));
  assert.equal(box.find("input")[1].checked, true);

  // A new phase is drawn afresh.
  paintPreFlight(box, { kind: "result", pre: pre() }, doc, state, acts([]));
  assert.equal(box.find("textarea").length + box.find("input").length, 0);
});

test("the overlay says its status in one live region that outlives its redraws and exists before its words change (#452's review)", async () => {
  const { El } = await import("./dom.mjs");
  const box = new El("div");
  const later = () => new Promise((done) => setTimeout(done, 150));
  const region = () => box.childNodes.filter((node) => node instanceof El && node.getAttribute("role") === "status");
  const all = (node) => [node, ...node.childNodes.filter((c) => c instanceof El).flatMap(all)];
  paintPreFlight(box, { kind: "start", issues: null }, doc, state, acts([]));
  const [live] = region();
  assert.ok(live, "the overlay has a status region");
  assert.equal(region().length, 1);
  assert.equal(live.text, "", "the region is drawn empty, so its first words are a change a screen reader hears");
  await later();
  assert.equal(live.text, "Reading the open issues…");
  paintPreFlight(box, { kind: "predicting" }, doc, state, acts([]));
  assert.deepEqual(region(), [live], "the same region, still in the overlay");
  assert.equal(live.text, "Asking the writer which parts this will cross…");
  paintPreFlight(box, { kind: "pick", why: "the writer is not set" }, doc, state, acts([]));
  assert.deepEqual(region(), [live]);
  assert.equal(live.text, "No prediction: the writer is not set. Pick the parts by hand.");
  paintPreFlight(box, { kind: "result", pre: pre() }, doc, state, acts([]));
  assert.deepEqual(region(), [live]);
  assert.equal(live.text, "");
  assert.equal(all(box).filter((node) => node.getAttribute("role") === "status").length, 1, "no other line is a status");
});

// Closing the overlay empties its box (`paint`) and leaves the region detached with its last words; opened again, the region is drawn
// empty once more, so its words are a change a screen reader hears, never a stale line (#452's outside review).
test("the overlay opened again draws its status region empty before its words", async () => {
  const { El } = await import("./dom.mjs");
  const box = new El("div");
  const later = () => new Promise((done) => setTimeout(done, 150));
  const region = () => box.childNodes.find((node) => node instanceof El && node.getAttribute("role") === "status");
  paintPreFlight(box, { kind: "start", issues: null }, doc, state, acts([]));
  await later();
  assert.equal(region().text, "Reading the open issues…");
  box.replaceChildren();
  paintPreFlight(box, { kind: "pick", why: "the writer is not set" }, doc, state, acts([]));
  assert.equal(region().text, "", "the region opened again is empty");
  await later();
  assert.equal(region().text, "No prediction: the writer is not set. Pick the parts by hand.");
});
