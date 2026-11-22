// A folded turn keeps one title row for each card Daniel has not seen (danielrispler/plotkeeper#211). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument, tauri } from "./dom.mjs";

const calls = [];
tauri(calls);
// The document is set after the import: times.ts starts a timer when one exists at load.
const { foldedTurn } = await import("../../dist/story.js");
globalThis.document = fakeDocument({ visibilityState: "visible", body: { dataset: {} } });

const stored = (id, title, shown_at = null) => ({
  request: { session_id: "s", id, agent: null, changed_by: {}, check: null },
  card: { summary: title, explanation: "e", paths: [], code: "", why_it_matters: "", not_checked: "", architecture: null, quiz: null },
  guess: null,
  check_result: null,
  shown_at,
});
const turn = { id: null, prompt: "Fix it", message: "Done.", checks: [], started_at: 1, ended_at: 5, files: null };
const rows = (cards) => {
  const node = foldedTurn({ id: "s" }, { index: 0, turn, cards, checks: [], agents: [], questions: [], blocked: [] });
  return node.find("details").filter((d) => d.className.includes("unseen-card"));
};

test("a folded turn's unseen card is one title row; a seen card has none, two unseen cards have two, no card has none", () => {
  assert.deepEqual(rows([stored("s@2", "Delete shows on hover")]).map((r) => r.find("summary")[0].text), ["Delete shows on hover"]);
  assert.equal(rows([stored("s@3", "Seen", 1234)]).length, 0);
  assert.equal(rows([stored("s@4", "One"), stored("s@5", "Two")]).length, 2);
  assert.equal(rows([]).length, 0);
});

test("opening a row builds the card in place and writes its shown tap", () => {
  const [row] = rows([stored("s@6", "Opened in place")]);
  assert.ok(row, "the unseen card has a row");
  assert.equal(row.find("article").length, 0);
  row.open = true;
  row.listeners.toggle();
  assert.equal(row.find("article").length, 1);
  assert.deepEqual(calls.filter(([c]) => c === "shown"), [["shown", { ids: ["s@6"] }]]);
});
