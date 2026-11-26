// The part sheet drawn with Explanations on and off (spec #520, ticket #522), run on the `tsc` output: the Agent plugins part, with
// links of two kinds and two not yet ratified, through `sheetCard`, the card the sheet puts in its slot. Run: `npm test`.
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
const { hintOf } = await import("../../dist/dom.js");

const all = (node) => [node, ...node.childNodes.filter((child) => child instanceof El).flatMap(all)];
const classed = (node, name) => all(node).filter((each) => each.className.split(" ").includes(name));
const lines = (node) => classed(node, "explain");
const byId = (node, id) => all(node).find((each) => each.id === id);
// What a node is described by besides its own hint (spec #582): the explanation lines.
const explainedBy = (each) => each.getAttribute("aria-describedby") !== null && each.getAttribute("aria-describedby") !== each.getAttribute("interestfor");
const described = (node) => all(node).filter(explainedBy);

/** @type {import("../../dist/map-model.js").Part} */
const plugins = {
  id: "plugins",
  name: "Agent plugins",
  caption: "One per agent",
  purpose: "Reads each agent's events.",
  tier: "core",
  paths: ["agents/**"],
  holds: [
    { name: "Plugins", text: "One per agent.", paths: ["agents/claude/plugin.json"] },
    { name: "Rendering", text: "How a plugin's output is drawn." },
  ],
};
// Two kinds, file and spawn, each twice; the first and the third not yet ratified.
const links = [
  { id: "plugins-hook", from: "plugins", to: "hook", kind: "file", reason: "reads its events" },
  { id: "server-plugins", from: "server", to: "plugins", kind: "spawn" },
  { id: "plugins-data", from: "plugins", to: "data", kind: "file" },
  { id: "plugins-agent", from: "plugins", to: "agent", kind: "spawn" },
];
// The view with only what the sheet reads; the rest of a `MapView` is left out.
/** @returns {import("../../dist/map.js").MapView} */
const view = (state = {}) => /** @type {any} */ ({
  project: "/w/app",
  scrub: null,
  positions: {},
  slots: {},
  doc: {
    parts: [plugins, ...["hook", "server", "data", "agent"].map((id) => ({ id, name: id, caption: "", purpose: "", holds: [], tier: "core", paths: [] }))],
    actors: [],
    links,
    laws: [{ id: "plugins-order", on: "plugins", text: "Events apply in order", backing: "check" }],
  },
  state: {
    user: { tiers: {}, fog: { parts: {} } },
    ratify: [
      { pr: 12, kind: "link", id: "plugins-hook" },
      { pr: 12, kind: "link", id: "plugins-data" },
    ],
    ci: null,
    fog: {},
    catch_ups: {},
    law_patches: [],
    ...state,
  },
});
const explanations = (on) => {
  studio.files = { ...studio.files, settings: { ...studio.files.settings, explanations: on } };
};

test("on: each fold, the selected tier and the links carry their lines, kinds and not yet ratified at their first link only", () => {
  explanations(true);
  const { card, toggle } = sheetCard(view(), plugins);
  assert.equal(toggle.getAttribute("aria-pressed"), "true");
  assert.equal(hintOf(toggle)?.textContent, "Hide explanations");
  // Each fold's summary is its heading (#637); the Fog section's line is the catch-up card's (#636).
  const folds = classed(card, "sheet-fold").filter((fold) => !fold.hidden);
  const summaries = folds.map((fold) => classed(fold, "sheet-fold-summary")[0]);
  assert.deepEqual(summaries.map((s) => classed(s, "sheet-fold-title")[0].text), ["Laws", "Talks to", "Holds", "Tier", "CI on main", "History"]);
  for (const summary of summaries) {
    const line = byId(card, summary.getAttribute("aria-describedby"));
    assert.ok(line, `"${summary.text}" points at its line`);
  }
  assert.equal(byId(card, summaries[3].getAttribute("aria-describedby")).text, SHEET.tier);
  // The selected tier, Core, is described by its meaning; the others are not.
  const tiers = classed(card, "sheet-tiers")[0].childNodes.filter((child) => child instanceof El);
  const pressed = tiers.find((button) => button.getAttribute("aria-pressed") === "true");
  assert.equal(byId(card, pressed.getAttribute("aria-describedby")).text, SHEET.tiers.core);
  assert.equal(tiers.filter(explainedBy).length, 1);
  // Talks to: file's and not yet ratified's at the first link, spawn's at the second, nothing at the third or fourth.
  const rows = classed(card, "sheet-link");
  assert.equal(rows.length, 4);
  assert.equal(classed(card, "ratify-mark").length, 2, "two links are not yet ratified");
  assert.deepEqual(rows.map((row) => lines(row).map((line) => line.text)), [[SHEET.kinds.file, SHEET.notYet], [SHEET.kinds.spawn], [], []]);
  // The held item with no paths is a note.
  assert.deepEqual(classed(card, "sheet-held-note").map((row) => classed(row, "sheet-held-name")[0].text), ["Rendering"]);
});

test("off: no line is drawn, nothing points at one, and the ? is not pressed", () => {
  explanations(false);
  const { card, toggle } = sheetCard(view(), plugins);
  assert.equal(toggle.getAttribute("aria-pressed"), "false");
  assert.equal(hintOf(toggle)?.textContent, "Show explanations");
  assert.deepEqual(lines(card), []);
  assert.deepEqual(described(card), []);
  assert.equal(classed(card, "sheet-link").length, 4);
  assert.equal(classed(card, "sheet-held-note").length, 1, "a note is a note either way");
});

// The line under the fog buttons, in the catch-up card (#636), and the buttons it sits under, for a fogged part with `offer`.
const fogLine = (offer) => {
  explanations(true);
  const { card } = sheetCard(view({ fog: { plugins: "thick" }, catch_ups: offer ? { plugins: offer } : {} }), plugins);
  const fog = classed(card, "catch-up-card")[0];
  const row = classed(fog, "catch-up-actions")[0];
  const buttons = row.childNodes.filter((child) => child instanceof El && child.tag === "button");
  const line = fog.childNodes[fog.childNodes.indexOf(row) + 1];
  return { line, buttons, card };
};

test("on: a line sits under the fog buttons whatever they are, saying what the buttons shown do, and the buttons point at it", () => {
  const catchUp = fogLine({ kind: "catch-up", ready: false, writing: false, failed: false });
  assert.deepEqual(catchUp.buttons.map((b) => b.text), ["Catch up", "I know this"]);
  assert.equal(catchUp.line?.text, SHEET.fogActions.catchUp);

  const chapter = fogLine({ kind: "chapter", pr: 12, chapter: 1 });
  assert.deepEqual(chapter.buttons.map((b) => b.text), ["Read #12's brief", "I know this"]);
  assert.equal(chapter.line?.text, SHEET.fogActions.chapter);

  const alone = fogLine(null);
  assert.deepEqual(alone.buttons.map((b) => b.text), ["I know this"]);
  assert.equal(alone.line?.text, SHEET.fogActions.know);

  for (const { line, buttons } of [catchUp, chapter, alone]) {
    assert.ok(line.id, "the line has an id");
    for (const button of buttons) assert.equal(button.getAttribute("aria-describedby"), line.id);
  }
  explanations(false);
  const { card } = sheetCard(view({ fog: { plugins: "thick" } }), plugins);
  assert.deepEqual(lines(card), []);
  assert.deepEqual(described(card), []);
});

test("a law's mark on the sheet is a mark, its hint the law's backing and text as the row's title said, keyed apart from the plate's", () => {
  const { card } = sheetCard(view(), plugins);
  const [mark] = classed(card, "law-mark");
  assert.equal(mark.getAttribute("role"), "img");
  assert.equal(mark.getAttribute("aria-label"), "Backed by a check: Events apply in order");
  assert.equal(hintOf(mark)?.textContent, "Backed by a check: Events apply in order");
  assert.equal(mark.getAttribute("data-key"), "sheet-plugins-plugins-order");
  assert.equal(mark.getAttribute("tabindex"), null);
});
