// The part sheet's folds under the catch-up card (spec #633, ticket #637), run on the `tsc` output through `sheetCard` on the stand-in DOM:
// the purpose clamped with More/Less, the chips, and the sections folded, each on its own, ordered Laws, Talks to, This session here, Holds,
// Tier, CI on main, History, with CI on main first and open when a run breaks one of the part's laws. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument, tauri } from "./dom.mjs";

tauri([], (command) => (command === "map_history" ? [] : null));
globalThis.document = fakeDocument({ body: new El("body") });
// The stand-in media query: never reduced motion.
globalThis.matchMedia = () => /** @type {any} */ ({ matches: false, addEventListener() {} });
// The page store: a stand-in `localStorage` the tests fill and read.
const stored = new Map();
globalThis.localStorage = /** @type {any} */ ({
  getItem: (key) => stored.get(key) ?? null,
  setItem: (key, value) => void stored.set(key, String(value)),
  removeItem: (key) => void stored.delete(key),
});
const { sheetCard, startMapSheet } = await import("../../dist/map-sheet.js");
const { SHEET } = await import("../../dist/explanations.js");
const { drawStrip } = await import("../../dist/map-history.js");
const { studio } = await import("../../dist/state.js");

const all = (node) => [node, ...node.childNodes.filter((child) => child instanceof El).flatMap(all)];
const classed = (node, name) => all(node).filter((each) => each.className.split(" ").includes(name));
const byId = (node, id) => all(node).find((each) => each.id === id);

/** @type {import("../../dist/map-model.js").Part} */
const store = {
  id: "store",
  name: "Store",
  caption: "Keeps the map state",
  purpose: "Reads and writes each project's state.",
  tier: "core",
  paths: ["store/**"],
  holds: [{ name: "State file", text: "One per project.", paths: ["store/state.rs"] }],
};
const laws = [
  { id: "store-atomic", on: "store", text: "A write is atomic", backing: "check" },
  { id: "store-lazy", on: "store", text: "Reads are lazy", backing: "trust" },
];
const links = [{ id: "server-store", from: "server", to: "store", kind: "import" }];
const ciOf = (status) => ({ status, parts: status === "broken" ? ["store"] : [], earlier: false, tail: status === "broken" ? "test failed" : null, run_url: "https://example.test/run/1", at: "2026-11-25T10:00:00Z" });
// The view with only what the sheet reads; `ci` is main's last run, null for none read.
/**
 * @param {{ ci?: any, scrub?: any }} [given]
 * @returns {import("../../dist/map.js").MapView}
 */
const view = ({ ci = null, scrub = null } = {}) => /** @type {any} */ ({
  project: "/w/app",
  scrub,
  positions: {},
  slots: {},
  doc: { parts: [store, { id: "server", name: "Server", caption: "", purpose: "", holds: [], tier: "core", paths: [] }], actors: [], links, laws },
  state: { source: null, user: { tiers: {}, fog: { parts: {} } }, ratify: [], ci, fog: {}, catch_ups: {}, law_patches: [] },
});
const explanations = (on) => {
  studio.files = { ...studio.files, settings: { ...studio.files.settings, explanations: on } };
};
// The sheet's folds that show, each its title.
const folds = (card) => classed(card, "sheet-fold").filter((fold) => !fold.hidden);
const titles = (card) => folds(card).map((fold) => classed(fold, "sheet-fold-title")[0].text);
// No fold kept open. An empty store is not used: a browser that keeps nothing reads as empty, and the page then keeps what was opened.
const fresh = () => void stored.set("map-sheet-folds", "[]");
const fold = (card, title) => folds(card).find((each) => classed(each, "sheet-fold-title")[0].text === title);

// "This session here" shows only with a session's story open beside the sheet: the page's deps say which panel shows.
let story = false;
const session = { id: "s1", turns: [] };
startMapSheet({ besideSheet() {}, panelShown: () => (story ? { kind: "story", session: "s1" } : null), headline: () => null, show() {} });

test("the folds run Laws, Talks to, Holds, Tier, CI on main, History; with a story open This session here comes third", () => {
  explanations(false);
  story = false;
  assert.deepEqual(titles(sheetCard(view({ ci: ciOf("passed") }), store).card), ["Laws", "Talks to", "Holds", "Tier", "CI on main", "History"]);
  story = true;
  studio.selection = /** @type {any} */ ({ session, agent: null });
  assert.deepEqual(titles(sheetCard(view({ ci: ciOf("passed") }), store).card), ["Laws", "Talks to", "This session here", "Holds", "Tier", "CI on main", "History"]);
  story = false;
  studio.selection = null;
});

test("a run that breaks one of the part's laws moves CI on main first, open, and the other folds stay folded", () => {
  explanations(false);
  fresh();
  const { card } = sheetCard(view({ ci: ciOf("broken") }), store);
  assert.deepEqual(titles(card), ["CI on main", "Laws", "Talks to", "Holds", "Tier", "History"]);
  assert.deepEqual(folds(card).map((each) => each.open), [true, false, false, false, false, false]);
  assert.ok(classed(fold(card, "CI on main"), "sheet-fold-value")[0].className.includes("bad"));
});

test("CI on main keeps its line and See the CI log for every run status it shows, and a part with no check-backed law and no break has none", () => {
  explanations(false);
  const cases = {
    passed: "CI on main passed",
    broken: "A test that backs laws here failed on main",
    "not-run": "CI on main failed, but the law tests did not run",
    unreadable: "CI on main failed and its log cannot be read",
  };
  for (const [status, line] of Object.entries(cases)) {
    const ci = fold(sheetCard(view({ ci: ciOf(status) }), store).card, "CI on main");
    assert.equal(classed(ci, "sheet-ci-line")[0]?.text, line, status);
    assert.ok(all(ci).some((each) => each.tag === "button" && each.text === "See the CI log"), `${status} drills`);
  }
  // CI not read yet: its line, nothing to drill into.
  const unread = fold(sheetCard(view({ ci: null }), store).card, "CI on main");
  assert.equal(classed(unread, "sheet-ci-line")[0]?.text, "CI on main has not been read");
  assert.ok(!all(unread).some((each) => each.text === "See the CI log"));
  const trusted = { ...store, id: "trusted" };
  assert.ok(!titles(sheetCard(view({ ci: ciOf("passed") }), trusted).card).includes("CI on main"));
  // A past commit's sheet shows no CI.
  assert.ok(!titles(sheetCard(view({ ci: ciOf("passed"), scrub: { commit: "abc" } }), store).card).includes("CI on main"));
});

test("each fold opens on its own, and the folds Daniel opened are open again on the next sheet, from the page store", () => {
  explanations(false);
  fresh();
  // Nothing kept: all folded.
  const first = sheetCard(view({ ci: ciOf("passed") }), store).card;
  assert.ok(folds(first).every((each) => !each.open), "all folded");
  // No fold belongs to a group that closes the others.
  assert.ok(folds(first).every((each) => !each.hasAttribute("name")));
  // He opens Laws, then Talks to: both stay open.
  for (const title of ["Laws", "Talks to"]) {
    const each = fold(first, title);
    each.open = true;
    each.listeners.toggle();
  }
  assert.deepEqual(folds(first).filter((each) => each.open).map((each) => classed(each, "sheet-fold-title")[0].text), ["Laws", "Talks to"]);
  // Another part's sheet, or a reload: the same folds open.
  const again = sheetCard(view({ ci: ciOf("passed") }), store).card;
  assert.deepEqual(folds(again).filter((each) => each.open).map((each) => classed(each, "sheet-fold-title")[0].text), ["Laws", "Talks to"]);
  // He closes Laws: only Talks to is kept.
  const laws = fold(again, "Laws");
  laws.open = false;
  laws.listeners.toggle();
  assert.deepEqual(JSON.parse(stored.get("map-sheet-folds")), ["talks"]);
  // What a page store holds from before is what opens.
  stored.set("map-sheet-folds", JSON.stringify(["history", "tier"]));
  const kept = sheetCard(view({ ci: ciOf("passed") }), store).card;
  assert.deepEqual(folds(kept).filter((each) => each.open).map((each) => classed(each, "sheet-fold-title")[0].text), ["Tier", "History"]);
  fresh();
});

test("CI on main opened because a run broke a law is not kept open for later sheets", () => {
  explanations(false);
  fresh();
  const ci = fold(sheetCard(view({ ci: ciOf("broken") }), store).card, "CI on main");
  // The toggle the drawn open state fires.
  ci.listeners.toggle();
  assert.equal(fold(sheetCard(view({ ci: ciOf("passed") }), store).card, "CI on main").open, false);
});

test("the chips name the tier, the laws, the links and CI, and a chip's press opens its fold and brings it into view", () => {
  explanations(false);
  fresh();
  const { card } = sheetCard(view({ ci: ciOf("passed") }), store);
  const chips = classed(card, "sheet-chip");
  assert.deepEqual(chips.map((each) => each.text), ["Core", "2 laws", "1 link", "CI passing"]);
  const target = { "Core": "Tier", "2 laws": "Laws", "1 link": "Talks to", "CI passing": "CI on main" };
  for (const each of chips) {
    const opened = fold(card, target[each.text]);
    assert.equal(opened.open, false);
    each.listeners.click();
    assert.equal(opened.open, true, `${each.text} opens ${target[each.text]}`);
    assert.ok(opened.scrolledIntoView, `${each.text} brings ${target[each.text]} into view`);
  }
  // A part with no check-backed law and no break shows no CI chip.
  assert.deepEqual(classed(sheetCard(view({ ci: ciOf("passed") }), { ...store, id: "trusted" }).card, "sheet-chip").map((each) => each.text), ["Core", "No laws", "No links"]);
  fresh();
});

test("History and This session here draw only their body inside their fold: the fold's summary is the one heading", async () => {
  explanations(true);
  fresh();
  story = true;
  studio.selection = /** @type {any} */ ({ session, agent: null });
  const v = view({ ci: ciOf("passed") });
  const { card, extra } = sheetCard(v, store);
  drawStrip(v, "store", extra);
  await new Promise((done) => setTimeout(done, 0));
  const headings = all(card).filter((each) => /^h[1-6]$/.test(each.tag)).map((each) => each.text);
  assert.ok(!headings.includes("History"), "no History heading besides its fold's");
  assert.ok(!headings.includes("This session here"), "no This session here heading besides its fold's");
  assert.equal(classed(card, "sheet-heading").length, 0);
  // The strip and the session's turns sit in their folds' bodies.
  assert.equal(classed(fold(card, "History"), "hist").length, 1);
  assert.equal(classed(fold(card, "This session here"), "sheet-here").length, 1);
  story = false;
  studio.selection = null;
});

test("with explanations on, each fold's line is the first thing in its body, the summary described by it; off, there is none", () => {
  explanations(true);
  fresh();
  const { card } = sheetCard(view({ ci: ciOf("passed") }), store);
  const lines = { Laws: SHEET.laws, "Talks to": SHEET.talksTo, Holds: SHEET.holds, Tier: SHEET.tier, "CI on main": SHEET.ci, History: SHEET.history };
  for (const [title, words] of Object.entries(lines)) {
    const each = fold(card, title);
    const body = classed(each, "sheet-fold-body")[0];
    const first = body.childNodes.find((child) => child instanceof El);
    assert.equal(first?.text, words, `${title}'s line`);
    assert.equal(byId(card, classed(each, "sheet-fold-summary")[0].getAttribute("aria-describedby"))?.text, words);
  }
  explanations(false);
  const off = sheetCard(view({ ci: ciOf("passed") }), store).card;
  assert.ok(folds(off).every((each) => classed(each, "explain").length === 0));
  // Off, a summary is described only by its own hint.
  assert.ok(folds(off).every((each) => {
    const summary = classed(each, "sheet-fold-summary")[0];
    return summary.getAttribute("aria-describedby") === summary.getAttribute("data-hint");
  }));
});

test("the purpose sits under the card clamped to two lines with More, which shows it whole and turns to Less, and the next draw keeps it", () => {
  explanations(false);
  // Each child's last class: its own, after `btn` and `quiet`.
  const kids = (node) => node.childNodes.filter((child) => child instanceof El).map((each) => each.className.split(" ").at(-1));
  const { card } = sheetCard(view(), store);
  assert.deepEqual(kids(card).slice(3), ["clear", "clamped", "sheet-more", "sheet-chips", "sheet-folds"]);
  const purpose = classed(card, "sheet-purpose")[0];
  const more = classed(card, "sheet-more")[0];
  assert.equal(purpose.text, "Reads and writes each project's state.");
  assert.ok(purpose.className.includes("clamped"));
  assert.equal(more.text, "More");
  assert.equal(more.getAttribute("aria-expanded"), "false");
  assert.equal(more.getAttribute("aria-controls"), purpose.id);
  more.listeners.click();
  assert.ok(!purpose.className.includes("clamped"));
  assert.equal(more.text, "Less");
  assert.equal(more.getAttribute("aria-expanded"), "true");
  const again = sheetCard(view(), store).card;
  assert.ok(!classed(again, "sheet-purpose")[0].className.includes("clamped"), "a redraw keeps it whole");
  classed(again, "sheet-more")[0].listeners.click();
  assert.ok(classed(sheetCard(view(), store).card, "sheet-purpose")[0].className.includes("clamped"), "Less clamps it again");
});
