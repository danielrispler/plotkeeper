// Accept and Overturn on an item to ratify (spec #334, ticket #343), run on the `tsc` output: the overturn's note, and how its line is
// said (#465).
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument, tauri } from "./dom.mjs";

const calls = [];
tauri(calls);
globalThis.document = fakeDocument({ body: new El("body") });
const { mapCheckNode, overturnNote, ratifyActions, ratifyHead } = await import("../../dist/map-ratify.js");

const all = (node) => [node, ...node.childNodes.filter((child) => child instanceof El).flatMap(all)];
const statuses = (node) => all(node).filter((each) => each.getAttribute("role") === "status");
const later = (ms = 150) => new Promise((done) => setTimeout(done, ms));
const item = { pr: 12, kind: "law", id: "core-order", text: "Events apply in order", change: "added" };
const holds = (shown) => shown.pr === item.pr && shown.kind === item.kind && shown.id === item.id;

test("an overturn says its line once, in the page's action region; the note drawn with the sheet or brief is not live (#465)", async () => {
  const decided = [];
  const box = /** @type {any} */ (ratifyActions("/w/app", item, (state) => decided.push(state)));
  // The stand-in DOM has no selectors; the actions look up their own buttons.
  box.querySelectorAll = () => box.find("button");
  assert.equal(overturnNote("/w/app", holds), null, "no note before an overturn");
  box.find("button").find((button) => button.text === "Overturn").listeners.click();
  await later(10);
  assert.deepEqual(decided, ["overturned"]);
  assert.equal(calls.filter(([command]) => command === "ratify_set").length, 1, "the decision is recorded");
  // No clipboard here, as when a browser refuses it: the note holds the prompt to copy by hand.
  const note = /** @type {any} */ (overturnNote("/w/app", holds));
  assert.match(note.text, /Copy this prompt and paste it into your agent/);
  assert.deepEqual(statuses(note), [], "the note is drawn again with its sheet or brief, so it is not live");
  const [live] = statuses(document.body);
  assert.ok(live, "the page has an action region");
  assert.equal(live.text, "", "the region is empty before the words");
  await later();
  assert.equal(live.text, "Copy this prompt and paste it into your agent");
  // The sheet drawn again draws the note anew; the region keeps its node and says nothing new.
  assert.deepEqual(statuses(overturnNote("/w/app", holds)), []);
  assert.deepEqual(statuses(document.body), [live]);
  assert.equal(live.text, "Copy this prompt and paste it into your agent");
});

const elements = (node) => node.childNodes.filter((child) => child instanceof El);

test("Ratify's head holds its lines in one slot, and is absent with none (#641)", () => {
  assert.equal(ratifyHead([]), null);
  const one = /** @type {any} */ (document.createElement("p"));
  const two = /** @type {any} */ (document.createElement("p"));
  const head = /** @type {any} */ (ratifyHead([one, two]));
  assert.equal(head.className, "ratify-head");
  assert.deepEqual(elements(head), [one, two], "more than one line, in order");
});

test("the head's map check opens to its problems, stays open when the step is drawn again, and is absent when the check passes (#641)", () => {
  assert.equal(mapCheckNode("/w/app|12|cafe", []), null);
  const problems = ["stray/s.ts is tracked but no part owns it", "import link \"a-b\" runs from \"a\" to \"b\", but no import runs that way"];
  const node = /** @type {any} */ (mapCheckNode("/w/app|12|cafe", problems));
  assert.equal(node.tag, "details");
  const [summary, list] = elements(node);
  assert.equal(summary.tag, "summary", "the summary is the first child");
  assert.equal(summary.text, "The map check fails at this head: 2 problems");
  assert.equal(list.tag, "ul");
  assert.deepEqual(list.find("li").map((li) => li.text), problems);
  assert.equal(node.open, false, "closed at first");
  node.open = true;
  node.listeners.toggle();
  assert.equal(/** @type {any} */ (mapCheckNode("/w/app|12|cafe", problems)).open, true, "drawn again, it is still open");
  assert.equal(/** @type {any} */ (mapCheckNode("/w/app|13|beef", problems)).open, false, "another PR's opens closed");
});
