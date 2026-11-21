// A card's file chips open the file's real hunk, as its row in the evidence line does (danielrispler/plotkeeper#121). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument, tauri } from "./dom.mjs";

const calls = [];
const hunk = { text: "@@ -1 +1 @@\n-a\n+b\n", left_out: 0, binary: false, first_line: 1 };
tauri(calls, (command) => (command === "guess" ? new Promise(() => {}) : command === "turn_file_hunk" ? hunk : null));
// The document is set after the import: times.ts starts a timer when one exists at load.
const { cardLine, cardsFor, openCard, quizOpen } = await import("../../dist/cards.js");
const { fileRow } = await import("../../dist/evidence.js");
globalThis.document = fakeDocument({ visibilityState: "hidden" });

const stored = (card, id = "s1@1") => ({
  request: { session_id: "s1", id, agent: null, changed_by: {}, check: null },
  card: { summary: "", explanation: "e", paths: ["a.rs", "b.rs"], code: "a.rs:1-1\nfn a() {}", why_it_matters: "", not_checked: "", architecture: null, quiz: null, ...card },
  guess: null,
  check_result: null,
});

test("a chip of a file the turn changed opens the same hunk as the file's evidence row; another path is plain", async () => {
  const turn = { session: "s1", index: 4, files: [{ root: "/r", path: "a.rs", from: null, added: 1, removed: 1 }] };
  const node = cardLine(stored({}), turn);
  const chips = node.find("ul")[0].find("li");
  assert.equal(chips.length, 2);
  assert.equal(chips[1].find("button").length, 0);
  chips[0].find("button")[0].listeners.click();
  await new Promise((done) => setImmediate(done));
  assert.deepEqual(calls.find(([c]) => c === "turn_file_hunk"), ["turn_file_hunk", { session: "s1", turn: 4, root: "/r", path: "a.rs", from: null }]);
  assert.match(chips[0].find("pre")[0].text, /\+b/);
  assert.match(node.find("pre").at(-1).text, /fn a\(\) \{\}/);
});

test("a card with no code draws no code block", () => {
  const node = cardLine(stored({ code: "", paths: [] }), null);
  assert.equal(node.find("pre").length, 0);
});

test("a card's Quiz is hidden where a Quiz may not show beside a decision", () => {
  const quiz = { text: "Why?", key: 0, options: [{ text: "a", why: "b" }, { text: "c", why: "d" }] };
  const cards = [stored({ quiz })];
  assert.equal(cards.some(quizOpen), true);
  assert.equal(cardsFor(cards, true), cards);
  const hidden = cardsFor(cards, false);
  assert.equal(hidden.some(quizOpen), false);
  assert.equal(hidden[0].card.quiz, null);
  assert.equal(cards[0].card.quiz, quiz);
});

const quizOf = { text: "Why a stack?", key: 0, options: [{ text: "It nests.", why: "Right." }, { text: "It is fast.", why: "No." }] };
const press = (key, value = "") => ({ key, target: { value }, preventDefault() {} });
const buttons = (node, pattern) => node.find("button").find((b) => pattern.test(b.text));

test("a card answered in this view stays open on the redraw after its pick", () => {
  const open = stored({ quiz: quizOf }, "s1@10");
  buttons(cardLine(open), /It nests/).listeners.click();
  const answered = cardLine({ ...open, guess: { option: 0, right: true } });
  assert.equal(answered.tag, "details");
  assert.equal(answered.open, true);
  // Another answered card is one closed line, until a citation opens it.
  const other = { ...stored({ quiz: quizOf }, "s1@11"), guess: { option: 1, right: false } };
  assert.equal(cardLine(other).open, false);
  openCard("s1@11");
  assert.equal(cardLine(other).open, true);
});

test("a card's file chip opens on its own, not with the evidence row of the same file", async () => {
  const file = { root: "/r", path: "a.rs", from: null, added: 1, removed: 1 };
  const turn = { session: "s9", index: 2, files: [file] };
  const node = cardLine(stored({ paths: ["a.rs"] }, "s9@2"), turn);
  node.find("li")[0].find("button")[0].listeners.click();
  await new Promise((done) => setImmediate(done));
  const row = fileRow("s9", 2, { kind: "file", label: "a.rs", file });
  assert.equal(row.find("pre")[0].hasAttribute("hidden"), true);
});

test("during the trial a card's Quiz asks the confidence tap, then the why line, and the reveal waits for it", () => {
  const open = stored({ quiz: quizOf }, "s1@20");
  const first = cardLine(open, null, true);
  assert.ok(buttons(first, /^Sure/) && buttons(first, /^Not sure/));
  buttons(first, /It nests/).listeners.click();
  const picked = { ...open, guess: { option: 0, right: true } };
  const waiting = cardLine(picked, null, true);
  const [input] = waiting.find("input");
  assert.equal(input.placeholder, "In a few words, why?");
  assert.doesNotMatch(waiting.text, /Why it matters/);
  input.listeners.keydown(press("Enter", "it nests"));
  assert.deepEqual(calls.at(-1), ["quiz_tap", { id: "s1@20", tap: { kind: "why", text: "it nests" } }]);
  assert.match(cardLine(picked, null, true).text, /Why it matters/);
  // Without the trial, none of it shows.
  assert.equal(cardLine(stored({ quiz: quizOf }, "s1@21"), null, false).find("input").length, 0);
});

test("a card's file chip and the evidence row of the same file each keep their own Ask box", () => {
  const file = { root: "/r", path: "a.rs", from: null, added: 1, removed: 1 };
  const turn = { session: "s7", index: 1, files: [file], ended: true };
  const chip = cardLine(stored({ paths: ["a.rs"] }, "s7@1"), turn).find("li")[0];
  const row = fileRow("s7", 1, { kind: "file", label: "a.rs", file });
  const [chipAsk] = chip.find("details");
  const [rowAsk] = row.find("details");
  assert.ok(chipAsk && rowAsk);
  assert.notEqual(chipAsk, rowAsk);
  assert.ok(chip.find("details").includes(chipAsk) && !row.find("details").includes(chipAsk));
});
