// The Brief's slots (danielrispler/plotkeeper#122). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { briefSlots } from "../../dist/cards.js";

const cards = { brief: null, goal: null, answers: [], open: [], recent: [], older: [], decisions: [] };

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
