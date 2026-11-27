// The Keep it current card (spec #753, ticket #766), run on the `tsc` output: when it opens, and the two snippets it holds.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument, tauri } from "./dom.mjs";

tauri([]);
globalThis.document = fakeDocument({ body: new El("body") });
const { askKeep, drawKeep, startKeep, agentsLine, actionsStep } = await import("../../dist/map-keep.js");

const all = (node) => [node, ...node.childNodes.filter((child) => child instanceof El).flatMap(all)];
const page = () => ({ box: /** @type {any} */ (document.createElement("div")), button: /** @type {any} */ (document.createElement("button")) });
const project = "/w/app";

test("the Keep it current button shows only while a map is drawn, and the card stays shut until it is asked for", () => {
  const { box, button } = page();
  drawKeep(box, button, project, false);
  assert.equal(button.hidden, true, "no map drawn, no button");
  assert.equal(box.hidden, true);
  drawKeep(box, button, project, true);
  assert.equal(button.hidden, false, "a map is drawn");
  assert.equal(box.hidden, true, "the card is not open unasked");
});

test("the head's button opens the card, and a second press shuts it", () => {
  const { box, button } = page();
  const redraw = () => drawKeep(box, button, project, true);
  startKeep(button, () => project, redraw);
  button.listeners.click();
  assert.equal(box.hidden, false);
  assert.equal(button.getAttribute("aria-expanded"), "true");
  button.listeners.click();
  assert.equal(box.hidden, true);
  assert.equal(button.getAttribute("aria-expanded"), "false");
});

test("Accept opens the card for its project, and a map drawn with no card asked for hides it", () => {
  const { box, button } = page();
  askKeep(project);
  drawKeep(box, button, project, true);
  assert.equal(box.hidden, false, "shown after Accept");
  drawKeep(box, button, "/w/other", true);
  assert.equal(box.hidden, true, "another project's card is not open");
  drawKeep(box, button, project, false);
  assert.equal(box.hidden, true, "no map drawn, no card");
});

test("the card holds the line for the agent instructions and the Actions step, each with Copy, and the exclude note", () => {
  const { box, button } = page();
  askKeep(project);
  drawKeep(box, button, project, true);
  const nodes = all(box);
  const snippets = nodes.filter((node) => node.tag === "pre").map((node) => node.text);
  assert.deepEqual(snippets, [agentsLine, actionsStep]);
  assert.equal(agentsLine, "When you change files, update `system-map.json` and run `npx -y plotkeeper map check` until it passes.");
  assert.match(actionsStep, /actions\/checkout@v\d+/);
  assert.match(actionsStep, /actions\/setup-node@v\d+/);
  assert.match(actionsStep, /run: npx -y plotkeeper map check/);
  assert.equal(nodes.filter((node) => node.getAttribute("aria-label")?.startsWith("Copy")).length, 2);
  assert.match(box.text, /\.git\/info\/exclude/);
});
