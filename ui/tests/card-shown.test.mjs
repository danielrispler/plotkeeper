// A card counts as shown only when it is on screen (danielrispler/plotkeeper#211), and an opened row goes once its card is seen.
// Run: `npm test`.
import { beforeEach, test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument, tauri } from "./dom.mjs";

const calls = [];
tauri(calls, (command) => (command === "turn_summary" ? { summary: { summary: "", headline: "Done", lines: [{ kind: "ask", text: "Pick one" }], quiz: null } } : null));
// The document is set after the import: times.ts starts a timer when one exists at load.
import { createRequire } from "node:module";
globalThis.marked = createRequire(import.meta.url)("marked");
const { foldedTurn, drawFeed } = await import("../../dist/story.js");
const { cardNode, showCards, markShown } = await import("../../dist/cards.js");
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
  body: { dataset: {} },
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
  studio.selection = null;
});

test("unfolding a folded turn writes the shown tap of its unseen card", () => {
  const node = foldedTurn({ ...session, id: "a" }, block([stored("s@10", "Unfolded")]));
  assert.deepEqual(shown(), []);
  node.find("button")[0].listeners.click();
  assert.deepEqual(shown(), ["s@10"]);
});

test("a card drawn while the window is visible is tapped before a session switch drops it", () => {
  cardNode(stored("s@11", "Drawn"));
  showCards({ ...studio.cards, recent: [], older: [], workers: [] });
  assert.deepEqual(shown(), ["s@11"]);
});

test("a card drawn while the window is hidden and folded away before it is visible is never tapped", () => {
  document.visibilityState = "hidden";
  cardNode(stored("s@12", "Hidden"));
  const later = { ...turn, id: null, started_at: 20, ended_at: 30 };
  studio.selection = { session: { ...session, id: "b", turns: [turn, later] }, agent: null };
  studio.cards = { ...studio.cards, recent: [stored("s@12", "Hidden")], older: [], workers: [] };
  drawFeed();
  document.visibilityState = "visible";
  markShown();
  assert.deepEqual(shown(), []);
});

test("closing an opened row of a card that was seen removes the row", () => {
  const node = foldedTurn({ ...session, id: "c" }, block([stored("s@13", "Row")]));
  const row = node.find("details")[0];
  row.open = true;
  row.listeners.toggle();
  assert.deepEqual(shown(), ["s@13"]);
  row.open = false;
  row.listeners.toggle();
  assert.equal(row.removed, true);
});

test("opening an unseen row of a folded turn with an ask line shows the card without its quiz", async () => {
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
