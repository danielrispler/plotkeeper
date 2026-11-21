// A worker's card shows in its session's rail, in the turn the core placed it in (danielrispler/plotkeeper#212). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument, tauri } from "./dom.mjs";

tauri([]);
// The document is set after the import: times.ts starts a timer when one exists at load.
const { feedTurns, foldedTurn, railOf } = await import("../../dist/story.js");
globalThis.document = fakeDocument({ visibilityState: "visible", body: { dataset: {} } });

const stored = (id, title, agent = null, shown_at = null) => ({
  request: { session_id: "s", id, agent, changed_by: {}, check: null },
  card: { summary: title, explanation: "e", paths: [], code: "", why_it_matters: "", not_checked: "", architecture: null, quiz: null },
  guess: null,
  check_result: null,
  shown_at,
});
const turn = (started_at, prompt) => ({ id: null, prompt, message: "Done.", checks: [], started_at, ended_at: started_at + 5, files: null });
const turns = [turn(10, "Spawn the worker"), turn(30, "Rename it"), turn(50, "worker w1 returned")];
const none = { checks: [], agents: [], questions: [] };

test("a worker's card is in the block of the turn the core placed it in, not the turn its time falls in", () => {
  const worker = stored("w1@41", "The worker's change", "w1");
  const feed = feedTurns(turns, { ...none, cards: [stored("s@12", "Session card")], placed: [{ card: worker, turn: 2 }] });
  assert.deepEqual(feed.map((b) => b.cards.map((c) => c.request.id)), [["w1@41"], [], ["s@12"]]);
});

test("a worker's card with no turn goes by its time, and a card placed in no turn of the list goes in the last block", () => {
  const feed = feedTurns(turns, { ...none, cards: [], placed: [{ card: stored("w1@35", "Late", "w1"), turn: null }] });
  assert.deepEqual(feed.map((b) => b.cards.length), [0, 1, 0]);
});

test("the rail of a session holds its workers' cards; a worker's own view lists the cards it was given", () => {
  const worker = stored("w1@41", "The worker's change", "w1");
  const session = { turns, checks: [], agents: [], questions: [], status_only: false };
  const rail = railOf({ session, agent: null }, [], [{ card: worker, turn: 2 }]);
  assert.deepEqual(rail.blocks[0].cards.map((c) => c.request.id), ["w1@41"]);
  const own = railOf({ session, agent: "w1" }, [worker], []);
  assert.deepEqual(own.blocks.map((b) => b.cards.map((c) => c.request.id)), [["w1@41"]]);
});

test("on a folded turn an unseen worker's card shows its title row, and a seen one shows none", () => {
  const rows = (cards) => {
    const [block] = feedTurns(turns, { ...none, cards: [], placed: cards.map((card) => ({ card, turn: 2 })) });
    return foldedTurn({ id: "s" }, block).find("details").filter((d) => d.className.includes("unseen-card"));
  };
  assert.deepEqual(rows([stored("w1@41", "Worker's card", "w1")]).map((r) => r.find("summary")[0].text), ["Worker's card"]);
  assert.equal(rows([stored("w1@42", "Seen", "w1", 1234)]).length, 0);
});
