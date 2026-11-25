// The Brief's slots (danielrispler/plotkeeper#122). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { briefSlots } from "../../dist/cards.js";
import { citedTurn } from "../../dist/story.js";

/** @type {import("../../dist/types.js").SessionCards} */
const cards = { brief: null, goal: null, answers: [], open: [], design: [], decisions: [], risk_notes: [], decision_notes: [] };

test("the slots come in order: Goal, Decisions, Open, each only when it has something", () => {
  assert.deepEqual(briefSlots(cards), []);
  const slots = briefSlots({
    ...cards,
    goal: "Show the rail from recorded facts.",
    answers: [{ question: "Which store?", reply: "Refs" }],
    open: [{ kind: "check", text: "npm run check ended failed" }],
  });
  assert.deepEqual(slots, [
    { title: "Goal", lines: ["Show the rail from recorded facts."] },
    { title: "Decisions", lines: ["Which store? Refs"] },
    { title: "Open", lines: ["npm run check ended failed"] },
  ]);
});

test("a brief stored as prose shows as it is until there is a Goal", () => {
  const prose = { ...cards, brief: "It set out to move a file.\nIt moved it." };
  assert.deepEqual(briefSlots(prose), [{ title: "", lines: ["It set out to move a file.", "It moved it."] }]);
  assert.equal(briefSlots({ ...prose, goal: "Move the file." })[0].title, "Goal");
});

test("Design so far comes last, with each line's citation", () => {
  const design = [{ text: "Anchor the trees, because gc drops them.", cite: "s@5" }];
  const slots = briefSlots({ ...cards, goal: "Show the rail.", design });
  assert.deepEqual(slots.at(-1), { title: "Design so far", lines: [design[0].text], cites: ["s@5"] });
  assert.equal(briefSlots({ ...cards, goal: "Show the rail." }).length, 1);
});

test("a citation names the turn with its id, or the turn a card was written in", () => {
  const turns = [
    { id: "s@10", started_at: 5 },
    { id: "s@40", started_at: 30 },
  ];
  assert.equal(citedTurn(turns, "s@40"), 1);
  assert.equal(citedTurn(turns, "s@12"), 0);
  assert.equal(citedTurn(turns, "s@2"), null);
});
