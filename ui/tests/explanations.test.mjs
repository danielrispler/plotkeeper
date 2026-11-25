// The part sheet explains itself (spec #520, ticket #522), run on the `tsc` output: the texts, and the lines, links and switch the
// sheet draws with Explanations on and off. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { El, fakeDocument, tauri } from "./dom.mjs";

const calls = [];
tauri(calls, (command) => (command === "set_explanations" ? '{ "explanations": false }' : null));
globalThis.document = fakeDocument({ body: new El("body") });
const { BRIEF, BRIEF_HEADINGS, RATIFY, RATIFY_HEADINGS, SHEET, SHEET_HEADINGS, explained: explainedNodes, stepLine, explainedChoice, explainToggle: toggleNode, explainedLinks, explanationsApplied, onExplanations, startExplanations } = await import("../../dist/explanations.js");
// The page's nodes here are the stand-in DOM's, which the tests read as such.
/** @param {Parameters<typeof explainedNodes>} args @returns {any[]} */
const explained = (...args) => explainedNodes(...args);
/** @param {Parameters<typeof toggleNode>} args @returns {any} */
const explainToggle = (...args) => toggleNode(...args);
const { studio } = await import("../../dist/state.js");

// A word has a letter: the arrows → and ← are marks, not words.
const words = (line) => line.split(/\s+/).filter((word) => /\p{L}/u.test(word)).length;
const every = (value) => (typeof value === "string" ? [value] : Object.values(value).flatMap(every));
const later = () => new Promise((done) => setTimeout(done, 10));

test("every heading the sheet draws has a line, and every text is at most 12 words, to you", () => {
  const ui = new URL("../", import.meta.url);
  const drawn = ["map-sheet.ts", "map-history.ts"].flatMap((name) => [...readFileSync(new URL(name, ui), "utf8").matchAll(/text\("h3", "([^"]+)", "sheet-heading"\)|\bheading\("([^"]+)"\)/g)].map((m) => m[1] ?? m[2]));
  assert.ok(drawn.length >= 7, `found the sheet's headings: ${drawn}`);
  for (const heading of drawn) assert.ok(SHEET_HEADINGS[heading], `"${heading}" has no line`);
  for (const line of every(SHEET)) {
    assert.ok(words(line) <= 12, `over 12 words: ${line}`);
    assert.doesNotMatch(line, /Daniel/);
  }
  // Every link kind has its line, in the same form.
  for (const kind of ["file", "spawn", "http", "import"]) assert.match(SHEET.kinds[kind], new RegExp(`^${kind}: `));
});

test("every heading the brief's reader and Ratify draw has a line: each step kind the writer may send, Ratify's titles, Also touched", () => {
  const root = new URL("../../", import.meta.url);
  const read = (path) => readFileSync(new URL(path, root), "utf8");
  // The kinds the brief's writer is told it may send, and the part catch-up's, which are among them.
  const kinds = read("understanding/src/pr_brief.rs").match(/"k": one of ([^\n]+)/)[1].match(/"([^"]+)"/g).map((k) => k.slice(1, -1));
  assert.ok(kinds.length >= 7, `found the step kinds: ${kinds}`);
  for (const kind of kinds) assert.ok(BRIEF_HEADINGS[kind], `the step kind "${kind}" has no line`);
  // The Ratify step's titles, from the step `ratify.ts` makes.
  const titles = [...read("ui/ratify.ts").matchAll(/\bh: "([^"]+)"/g)].map((m) => m[1]);
  assert.deepEqual(titles.sort(), ["Accept or overturn", "Guard with a law"]);
  for (const title of titles) assert.ok(RATIFY_HEADINGS[title], `Ratify's "${title}" has no line`);
  // The reader's own headings.
  const drawn = [...read("ui/map-brief.ts").matchAll(/text\("h[1-6]", "([^"]+)"/g)].map((m) => m[1]);
  assert.ok(drawn.includes("Also touched"), `found the reader's headings: ${drawn}`);
  for (const heading of drawn) assert.ok(BRIEF_HEADINGS[heading], `"${heading}" has no line`);
  for (const line of [...every(BRIEF), ...every(RATIFY)]) {
    assert.ok(words(line) <= 12, `over 12 words: ${line}`);
    assert.doesNotMatch(line, /Daniel/);
  }
  // A meaning is worded once: Ratify's "not yet ratified" is the sheet's.
  assert.equal(RATIFY.notYet, SHEET.notYet);
});

test("a step's line is its kind's, and on the Ratify step its title's; a kind with none has no line", () => {
  assert.equal(stepLine({ k: "Could break", h: "The rail may drop a turn" }), BRIEF.steps["Could break"]);
  assert.equal(stepLine({ k: "Ratify", h: "Accept or overturn" }), RATIFY.accept);
  assert.equal(stepLine({ k: "Ratify", h: "Guard with a law" }), RATIFY.guard);
  assert.equal(stepLine({ k: "Something new", h: "A title" }), undefined);
});

// The Agent plugins part's links: two kinds, the first and third not yet ratified.
/** @type {{ kind?: import("../../dist/map-model.js").LinkKind; unratified: boolean }[]} */
const talks = [
  { kind: "file", unratified: true },
  { kind: "spawn", unratified: false },
  { kind: "file", unratified: true },
  { kind: "spawn", unratified: false },
];
const rowsOf = () => talks.map(() => new El("li"));
const notesIn = (row) => row.childNodes.filter((child) => child instanceof El && child.className.includes("explain")).map((note) => note.text);

test("with Explanations on, a heading, the selected tier and the links carry their lines, each kind and not yet ratified at its first link only", () => {
  const [heading, line] = explained(new El("h3", {}, ["Talks to"]), SHEET.talksTo, true);
  assert.equal(line.text, SHEET.talksTo);
  assert.ok(line.id);
  assert.equal(heading.getAttribute("aria-describedby"), line.id);

  const tiers = new El("div");
  const [core, iface] = [new El("button"), new El("button")];
  const [box, meaning] = /** @type {any[]} */ (explainedChoice(tiers, core, SHEET.tiers.core, true));
  assert.equal(box, tiers);
  assert.equal(meaning.text, SHEET.tiers.core);
  assert.equal(core.getAttribute("aria-describedby"), meaning.id);
  assert.equal(iface.getAttribute("aria-describedby"), null);

  const rows = explainedLinks(rowsOf(), talks, true);
  assert.deepEqual(rows.map(notesIn), [[SHEET.kinds.file, SHEET.notYet], [SHEET.kinds.spawn], [], []]);
});

test("with Explanations off, no line is drawn and nothing points at one", () => {
  const heading = new El("h3", {}, ["Tier"]);
  assert.deepEqual(explained(heading, SHEET.tier, false), [heading]);
  assert.equal(heading.getAttribute("aria-describedby"), null);
  const core = new El("button");
  const tiers = new El("div");
  assert.deepEqual(explainedChoice(tiers, core, SHEET.tiers.core, false), [tiers]);
  assert.equal(core.getAttribute("aria-describedby"), null);
  assert.deepEqual(explainedLinks(rowsOf(), talks, false).map(notesIn), [[], [], [], []]);
});

test("the ? beside the × is a toggle: pressed when on, its hover says what a click does, and a click writes the flipped setting", async () => {
  const on = explainToggle(true);
  assert.equal(on.getAttribute("aria-pressed"), "true");
  assert.equal(on.title, "Hide explanations");
  assert.equal(on.getAttribute("aria-label"), "Explanations");
  const off = explainToggle(false);
  assert.equal(off.getAttribute("aria-pressed"), "false");
  assert.equal(off.title, "Show explanations");

  const saved = [];
  startExplanations((text) => saved.push(text));
  studio.texts = { settings: "{}", keymap: null };
  on.listeners.click();
  await later();
  assert.deepEqual(calls.at(-1), ["set_explanations", { on: false, expected: "{}" }]);
  assert.deepEqual(saved, ['{ "explanations": false }'], "the written text applies at once");
});

test("the explained panels draw again only when the setting changes", () => {
  let drawn = 0;
  onExplanations(() => drawn++);
  const apply = (explanations) => {
    studio.files = { ...studio.files, settings: { ...studio.files.settings, explanations } };
    explanationsApplied();
  };
  apply(true);
  apply(true);
  assert.equal(drawn, 0, "the same setting draws nothing again");
  apply(false);
  assert.equal(drawn, 1);
  apply(true);
  assert.equal(drawn, 2);
});

test("a double click on the ? ends where two flips end, with no write refused: the second waits for the first's reply", async () => {
  // The server's settings write, as `settings.rs` has it: a write from text the file no longer holds is refused.
  let file = '{ "explanations": true }';
  const refused = [];
  const writes = [];
  // biome-ignore lint/suspicious/useAwait: a stand-in for an async API, so a throw in it rejects as the real one's would
  window.__TAURI__.core.invoke = /** @type {any} */ (async (command, args) => {
    if (command !== "set_explanations") return null;
    writes.push(args.on);
    if (args.expected !== file) {
      refused.push(args);
      throw new Error("settings.json changed since the page read it: try again");
    }
    file = JSON.stringify({ explanations: args.on });
    return file;
  });
  // The page applies the returned text, as `main.ts` does.
  startExplanations((settings) => {
    studio.texts = { settings, keymap: null };
    studio.files = { ...studio.files, settings: { ...studio.files.settings, explanations: JSON.parse(settings).explanations } };
  });
  studio.texts = { settings: file, keymap: null };
  studio.files = { ...studio.files, settings: { ...studio.files.settings, explanations: true } };
  const button = explainToggle(true);
  button.listeners.click();
  button.listeners.click();
  await later();
  assert.deepEqual(refused, [], "no write is refused");
  assert.equal(JSON.parse(file).explanations, true, "two flips end on again");
  assert.equal(studio.files.settings.explanations, true);
  // Three clicks end off, however many writes it takes.
  button.listeners.click();
  button.listeners.click();
  button.listeners.click();
  await later();
  assert.deepEqual(refused, []);
  assert.equal(JSON.parse(file).explanations, false);
  assert.equal(studio.files.settings.explanations, false);
});
