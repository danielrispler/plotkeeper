// A card's file chips open the file's real hunk, as its row in the evidence line does (danielrispler/plotkeeper#121). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El } from "./dom.mjs";

const calls = [];
const hunk = { text: "@@ -1 +1 @@\n-a\n+b\n", left_out: 0, binary: false, first_line: 1 };
globalThis.window = {
  __TAURI__: { core: { invoke: async (command, args) => (calls.push([command, args]), command === "turn_file_hunk" ? hunk : null) } },
};
// The document is set after the import: times.ts starts a timer when one exists at load.
const { cardLine, cardsFor, quizOpen } = await import("../../dist/cards.js");
globalThis.document = { createElement: (tag) => new El(tag), getElementById: () => new El("div"), visibilityState: "hidden" };

const stored = (card) => ({
  request: { session_id: "s1", id: "s1@1", agent: null, changed_by: {}, check: null },
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
