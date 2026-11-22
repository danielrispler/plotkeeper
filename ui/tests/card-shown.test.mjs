// A card counts as shown only when it is seen (danielrispler/plotkeeper#211, #306), and an opened row goes once its card is seen.
// Run: `npm test`.
import { afterEach, beforeEach, mock, test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument, tauri } from "./dom.mjs";

// A card is shown when seen (seen.ts): every observer made is kept, so a test can scroll its card into view.
const observers = [];
globalThis.IntersectionObserver = class {
  constructor(run) {
    this.run = run;
    observers.push(this);
  }
  observe() {}
  disconnect() {}
};
const scrollTo = (observer, element, viewport) =>
  observer.run([{ intersectionRatio: element, intersectionRect: { width: 100, height: viewport * 100 }, rootBounds: { width: 100, height: 100 } }]);

const calls = [];
tauri(calls, (command) => (command === "turn_summary" ? { summary: { summary: "", headline: "Done", lines: [{ kind: "ask", text: "Pick one" }], quiz: null } } : null));
// The document is set after the import: times.ts starts a timer when one exists at load.
import { createRequire } from "node:module";
globalThis.marked = createRequire(import.meta.url)("marked");
const { foldedTurn, drawFeed } = await import("../../dist/story.js");
const { cardNode, showCards } = await import("../../dist/cards.js");
const { studio } = await import("../../dist/state.js");
const document = fakeDocument({
  createElement: (tag) =>
    Object.assign(new El(tag), {
      querySelector: () => new El("span"),
      remove() {
        this.removed = true;
      },
    }),
  visibilityState: "visible",
  hasFocus: () => true,
  body: { dataset: {}, classList: { contains: () => false } },
});
globalThis.document = document;

const stored = (id, title, quiz = null) => ({
  request: { session_id: "s", id, agent: null, changed_by: {}, check: null },
  card: { summary: title, explanation: "e", paths: [], code: "", why_it_matters: "", not_checked: "", architecture: null, quiz },
  guess: null,
  check_result: null,
  shown_at: null,
});
const session = { id: "s", cwd: "/w", state: "idle", capabilities: { turn_trees: true, agent: "claude" }, transcript: { responses: [] }, turns: [], checks: [], agents: [], questions: [] };
const turn = { id: null, prompt: "Fix it", message: "Done.", checks: [], started_at: 1, ended_at: 5, files: null };
const block = (cards) => ({ index: 0, turn, cards, checks: [], agents: [], questions: [], blocked: [] });
const shown = () => calls.filter(([c]) => c === "shown").flatMap(([, a]) => a.ids);
beforeEach(() => {
  calls.length = 0;
  document.visibilityState = "visible";
  document.hasFocus = () => true;
  document.body.dataset = {};
  observers.length = 0;
  studio.selection = null;
  mock.timers.enable({ apis: ["setTimeout"] });
});

test("a card built but off screen is never tapped", () => {
  cardNode(stored("s@10", "Built"));
  showCards({ ...studio.cards, recent: [], older: [], workers: [] });
  mock.timers.tick(5000);
  assert.deepEqual(shown(), []);
});

test("unfolding a folded turn builds its card, tapped once it is seen", () => {
  const node = foldedTurn({ ...session, id: "a" }, block([stored("s@11", "Unfolded")]));
  node.find("button")[0].listeners.click();
  mock.timers.tick(5000);
  assert.deepEqual(shown(), []);
  scrollTo(observers.at(-1), 1, 0.4);
  mock.timers.tick(500);
  assert.deepEqual(shown(), ["s@11"]);
});

test("a card seen in a window that is unfocused or a folded rail is not tapped", () => {
  cardNode(stored("s@12", "Unfocused"));
  scrollTo(observers.at(-1), 1, 1);
  document.hasFocus = () => false;
  mock.timers.tick(5000);
  assert.deepEqual(shown(), []);
  document.hasFocus = () => true;
  document.body.dataset.layout = "focus";
  cardNode(stored("s@12b", "Folded"));
  scrollTo(observers.at(-1), 1, 1);
  mock.timers.tick(5000);
  assert.deepEqual(shown(), []);
});

test("a card taller than twice the viewport is tapped once half the viewport shows it", () => {
  cardNode(stored("s@13", "Tall"));
  scrollTo(observers.at(-1), 0.3, 0.5);
  mock.timers.tick(500);
  assert.deepEqual(shown(), ["s@13"]);
});

test("a card scrolled past in under 500 ms is not tapped", () => {
  cardNode(stored("s@15", "Passing"));
  scrollTo(observers.at(-1), 1, 1);
  mock.timers.tick(200);
  scrollTo(observers.at(-1), 0, 0);
  mock.timers.tick(5000);
  assert.deepEqual(shown(), []);
});

test("closing an opened row of a card that was seen removes the row", () => {
  const node = foldedTurn({ ...session, id: "c" }, block([stored("s@16", "Row")]));
  const row = node.find("details")[0];
  row.open = true;
  row.listeners.toggle();
  scrollTo(observers.at(-1), 1, 1);
  mock.timers.tick(500);
  assert.deepEqual(shown(), ["s@16"]);
  row.open = false;
  row.listeners.toggle();
  assert.equal(row.removed, true);
});

test("opening an unseen row of a folded turn with an ask line shows the card without its quiz", async () => {
  mock.timers.reset();
  const quiz = { text: "Which?", options: ["A", "B"], key: 0 };
  const withQuiz = { ...turn, id: "t-ask" };
  const make = () => foldedTurn({ ...session, id: "d" }, { ...block([stored("s@14", "Asked", quiz)]), turn: withQuiz });
  make();
  await new Promise((done) => setTimeout(done, 10));
  const [row] = make().find("details").filter((d) => d.className.includes("unseen-card"));
  row.open = true;
  row.listeners.toggle();
  assert.equal(row.find("article").length, 1);
  assert.equal(row.find("p").filter((p) => p.className === "quiz").length, 0);
});

afterEach(() => mock.timers.reset());
