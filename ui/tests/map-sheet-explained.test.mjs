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

test("on: each heading, the selected tier and the links carry their lines, kinds and not yet ratified at their first link only", () => {
  explanations(true);
  const { card, toggle } = sheetCard(view(), plugins);
  assert.equal(toggle.getAttribute("aria-pressed"), "true");
  assert.equal(hintOf(toggle)?.textContent, "Hide explanations");
  const headings = classed(card, "sheet-heading");
  assert.deepEqual(headings.map((h) => h.text), ["Holds", "Tier", "Laws", "Talks to", "Fog", "CI on main"]);
  for (const heading of headings) {
    const line = byId(card, heading.getAttribute("aria-describedby"));
    assert.ok(line, `"${heading.text}" points at its line`);
  }
  assert.equal(byId(card, headings[1].getAttribute("aria-describedby")).text, SHEET.tier);
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

// The line under the fog buttons and the buttons it sits under, for a fogged part with `offer`.
const fogLine = (offer) => {
  explanations(true);
  const { card } = sheetCard(view({ fog: { plugins: "thick" }, catch_ups: offer ? { plugins: offer } : {} }), plugins);
  const fog = classed(card, "sheet-fog")[0];
  const buttons = fog.childNodes.filter((child) => child instanceof El && child.tag === "button");
  const last = buttons.at(-1);
  const line = fog.childNodes[fog.childNodes.indexOf(last) + 1];
  return { line, buttons, card };
};

test("on: a line sits under the fog buttons whatever they are, saying what the buttons shown do, and the buttons point at it", () => {
  const catchUp = fogLine({ kind: "catch-up", ready: false, writing: false, failed: false });
  assert.deepEqual(catchUp.buttons.map((b) => b.text), ["Part catch-up", "I know this"]);
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
