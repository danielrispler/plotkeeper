// Cards are retired as a turn's or a worker's explanation (spec #334, ticket #348); the PR brief is. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fakeDocument, tauri } from "./dom.mjs";

tauri([], () => null);
// The document is set after the import: times.ts starts a timer when one exists at load.
const story = await import("../../dist/story.js");
const cards = await import("../../dist/cards.js");
const settings = await import("../../dist/settings.js");
globalThis.document = fakeDocument({});

const stored = (id) => ({
  request: { session_id: "s", id, agent: null, changed_by: {}, check: null },
  card: { summary: "An old card", explanation: "e", paths: [], code: "", why_it_matters: "", not_checked: "", architecture: null, quiz: null },
  guess: null,
  check_result: null,
  shown_at: null,
});
const turn = { id: null, prompt: "Fix it", message: "Done.", checks: [], started_at: 1, ended_at: 5, files: null };

test("no card shows in the rail, whatever the block holds, a folded turn or an open one", () => {
  const block = { index: 0, turn, cards: [stored("s@2")], checks: [], agents: [], questions: [], blocked: [] };
  const session = { id: "s", cwd: "/p", state: "idle", capabilities: { turn_trees: false }, turns: [turn, turn, turn] };
  const folded = story.foldedTurn(session, block);
  const unfolded = story.foldedTurn(session, { ...block, index: 1 });
  // The line's click opens the turn's block under it.
  const line = unfolded.children[0];
  const closed = unfolded.children.length;
  line.listeners.click();
  assert.ok(unfolded.children.length > closed, "the turn is open");
  for (const node of [folded, unfolded]) {
    assert.equal(node.find("article").length, 0);
    assert.equal(node.find("details").filter((d) => d.className.includes("unseen-card") || d.className.includes("answered-card")).length, 0);
  }
  const blocks = story.feedTurns([turn], { cards: [stored("s@2")], placed: [{ card: stored("s@3"), turn: 0 }], agents: [], questions: [] });
  assert.equal(blocks.some((b) => "cards" in b), false);
});

test("the card code is gone from the window", () => {
  for (const name of ["cardLine", "cardNode", "unseenRow", "rowCards", "quizState", "quizOpen", "quizAllowed", "cardsFor", "openCard", "checkVerdict", "unseen"]) {
    assert.equal(name in cards, false, name);
  }
  for (const file of ["../../ui/types.ts", "../../ui/story.ts"]) {
    const text = fs.readFileSync(new URL(file, import.meta.url), "utf8");
    assert.equal(/\bStoredCard\b|\bPlacedCard\b/.test(text), false, file);
  }
});

test("the settings hold no self-test day", () => {
  assert.equal("quiz_self_test" in settings.initial.settings, false);
});
