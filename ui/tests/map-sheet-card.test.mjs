// The catch-up card that leads the part sheet (spec #633, ticket #636), run on the `tsc` output through `sheetCard` on the stand-in DOM:
// under the part's name, caption and marks, a fogged part's card says how many changes landed and shows the waiting catch-up's outline
// (never its gist), with "Catch up · N steps" and "I know this"; a part with no fog shows "Caught up" and the Refresher. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument, tauri } from "./dom.mjs";

tauri([]);
globalThis.document = fakeDocument({ body: new El("body") });
// The stand-in media query: never reduced motion.
globalThis.matchMedia = () => /** @type {any} */ ({ matches: false, addEventListener() {} });
const { sheetCard } = await import("../../dist/map-sheet.js");
const { SHEET } = await import("../../dist/explanations.js");
const { studio } = await import("../../dist/state.js");

const all = (node) => [node, ...node.childNodes.filter((child) => child instanceof El).flatMap(all)];
const classed = (node, name) => all(node).filter((each) => each.className.split(" ").includes(name));
const byId = (node, id) => all(node).find((each) => each.id === id);
const kids = (node) => node.childNodes.filter((child) => child instanceof El);
const buttons = (node) => all(node).filter((each) => each.tag === "button").map((b) => b.text);

/** @type {import("../../dist/map-model.js").Part} */
const store = { id: "store", name: "Store", caption: "Keeps the map state", purpose: "Reads and writes each project's state.", tier: "core", paths: ["store/**"], holds: [] };
const entry = (n) => ({ commit: String(n).repeat(40), strength: "thin", pr: 600 + n, why: ["paths"] });
const outline = {
  title: "Store writes go atomic",
  steps: [
    { k: "Where", h: "The store keeps each project's map state" },
    { k: "Before", h: "Writes went straight to the state file" },
    { k: "Now", h: "Each write goes to a temp file, then a rename" },
  ],
};
/** @type {import("../../dist/types.js").CatchUpOffer} */
const ready = { kind: "catch-up", ready: true, writing: false, failed: false, outline };
// The view with only what the sheet reads; `fogged` gives the part three fog entries.
/**
 * @param {{ fogged?: boolean, offer?: any, scrub?: any, source?: string | null }} [given]
 * @returns {import("../../dist/map.js").MapView}
 */
const view = ({ fogged = true, offer = ready, scrub = null, source = null } = {}) => /** @type {any} */ ({
  project: "/w/app",
  scrub,
  positions: {},
  slots: {},
  doc: { parts: [store], actors: [], links: [], laws: [] },
  state: {
    source,
    user: { tiers: {}, fog: { parts: fogged ? { store: [entry(1), entry(2), entry(3)] } : {} } },
    ratify: [],
    ci: null,
    fog: fogged ? { store: "thin" } : {},
    catch_ups: fogged && offer ? { store: offer } : {},
    law_patches: [],
  },
});
const explanations = (on) => {
  studio.files = { ...studio.files, settings: { ...studio.files.settings, explanations: on } };
};
const cardOf = (v) => {
  const { card } = sheetCard(v, store);
  const found = classed(card, "catch-up-card");
  return { sheet: card, card: found[0], count: found.length };
};

test("a ready catch-up's card comes first after the name, caption and marks, with the outline and Catch up · 3 steps", () => {
  explanations(false);
  const { sheet, card, count } = cardOf(view());
  assert.equal(count, 1);
  const order = kids(sheet).map((each) => each.className.split(" ")[0]);
  assert.deepEqual(order.slice(0, 5), ["sheet-header", "sheet-name", "sheet-caption", "sheet-marks", "catch-up-card"]);
  assert.equal(classed(card, "catch-up-eyebrow")[0].text, "3 changes since you last caught up");
  assert.equal(classed(card, "catch-up-title")[0].text, "Store writes go atomic");
  const rows = classed(card, "catch-up-step").map((row) => [classed(row, "catch-up-kind")[0].text, classed(row, "catch-up-headline")[0].text]);
  assert.deepEqual(rows, [
    ["Where", "The store keeps each project's map state"],
    ["Before", "Writes went straight to the state file"],
    ["Now", "Each write goes to a temp file, then a rename"],
  ]);
  assert.deepEqual(buttons(card), ["Catch up · 3 steps", "I know this"]);
  assert.ok(classed(card, "sheet-catch-up")[0].className.includes("primary"), "the catch-up is the main button");
  // The Fog section is gone: the card holds what it held.
  assert.ok(!classed(sheet, "sheet-heading").some((h) => h.text === "Fog"));
  assert.equal(classed(sheet, "sheet-fog").length, 0);
});

test("the card shows only the outline's title and headlines: no step body and no gist, which stays behind Explain-back", () => {
  explanations(false);
  // A payload that carried more than the outline: none of it reaches the sheet.
  const steps = outline.steps.map((step) => ({ ...step, b: "the step's body" }));
  const { sheet } = cardOf(view({ offer: { ...ready, outline: { ...outline, steps, gist: "the whole story in one sentence" } } }));
  assert.ok(!sheet.textContent.includes("the whole story"), "no gist");
  assert.ok(!sheet.textContent.includes("the step's body"), "no step body");
});

test("a catch-up being written says the call that runs on its card, with no outline, and a failed one says to press again", () => {
  explanations(false);
  const writing = cardOf(view({ offer: { kind: "catch-up", ready: false, writing: true, failed: false, stage: { kind: "checking" } } })).card;
  assert.equal(classed(writing, "catch-up-eyebrow")[0].text, "3 changes since you last caught up");
  assert.equal(classed(writing, "sheet-catch-up-note")[0].text, "Checking claims…");
  assert.equal(classed(writing, "catch-up-steps").length, 0);
  assert.deepEqual(buttons(writing), ["Catch up", "I know this"]);
  const failed = cardOf(view({ offer: { kind: "catch-up", ready: false, writing: false, failed: true } })).card;
  assert.equal(classed(failed, "sheet-catch-up-note")[0].text, "The last write failed; press to try again");
  assert.deepEqual(buttons(failed), ["Catch up", "I know this"]);
});

test("while a newer catch-up is written, the card offers the one Daniel has, saying what it covers, and pressing it asks for that one", async () => {
  explanations(false);
  const older = { outline, fog: [entry(1).commit, entry(2).commit] };
  const writing = { kind: "catch-up", ready: false, writing: true, failed: false, older };
  const { card } = cardOf(view({ offer: writing }));
  assert.deepEqual(buttons(card), ["Catch up", "I know this", "Read the one you have"]);
  const read = all(card).find((each) => each.tag === "button" && each.text === "Read the one you have");
  const line = byId(card, read.getAttribute("aria-describedby"));
  assert.equal(line?.text, "Covers 2 of 3 changes: #601, #602");
  // The older one's outline stays off the card: its title and steps are the ready one's place.
  assert.equal(classed(card, "catch-up-steps").length, 0);
  const calls = [];
  tauri(calls);
  // The open's slow timer; the answer is no catch-up, so nothing plays.
  globalThis.window.setTimeout = setTimeout;
  read.listeners.click();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(calls[0], ["part_catch_up", { project: "/w/app", part: "store", older: true }]);
  tauri([]);
  // No older one stored: no such button.
  const alone = cardOf(view({ offer: { ...writing, older: undefined } })).card;
  assert.deepEqual(buttons(alone), ["Catch up", "I know this"]);
});

test("a part fogged by one PR whose brief lights it opens that chapter from the card", () => {
  explanations(false);
  const { card } = cardOf(view({ offer: { kind: "chapter", pr: 612, chapter: 1 } }));
  assert.deepEqual(buttons(card), ["Read #612's brief", "I know this"]);
  assert.equal(classed(card, "catch-up-steps").length, 0);
});

test("a part with no fog shows Caught up and the Refresher in the card, and the refresher opens inside it", () => {
  explanations(false);
  const source = JSON.stringify({ parts: [store], links: [], flows: [] });
  const v = view({ fogged: false, source });
  const { card } = cardOf(v);
  assert.equal(classed(card, "catch-up-eyebrow")[0].text, "Caught up");
  assert.deepEqual(buttons(card), ["Refresher"]);
  // Pressed, the refresher opens in the card, in the button's place.
  classed(card, "sheet-refresher")[0].listeners.click();
  assert.equal(classed(card, "refresher").length, 1, "the refresher is in the card");
  assert.equal(classed(card, "sheet-refresher").length, 0, "its button goes once it shows");
  // The reload draws the sheet again with it open, still in the card.
  const again = cardOf(v).card;
  assert.equal(classed(again, "refresher").length, 1);
  assert.equal(classed(again, "sheet-refresher").length, 0);
});

test("a past commit's sheet has no card", () => {
  explanations(false);
  assert.equal(cardOf(view({ scrub: { commit: "abc" } })).count, 0);
  assert.equal(cardOf(view({ fogged: false, scrub: { commit: "abc" } })).count, 0);
});

test("with explanations on, the card carries the fog's line under its eyebrow and the fog actions' line under its buttons", () => {
  explanations(true);
  const { card } = cardOf(view());
  const eyebrow = classed(card, "catch-up-eyebrow")[0];
  assert.equal(byId(card, eyebrow.getAttribute("aria-describedby"))?.text, SHEET.fog);
  const main = classed(card, "sheet-catch-up")[0];
  assert.equal(byId(card, main.getAttribute("aria-describedby"))?.text, SHEET.fogActions.catchUp);
  explanations(false);
});
