// One-line rows (danielrispler/Falinks#572): status by shape, a word with every glyph, colour only for needs you, distinct
// strip labels and the needs-you chip. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { chipText, secondLine, status, stripLabels } from "../../dist/list.js";

test("each state has its glyph and a word for its title and accessible name", () => {
  assert.deepEqual(status("needs_you"), ["pill", "Needs you"]);
  assert.deepEqual(status("running"), ["spinner", "Running"]);
  assert.deepEqual(status("your_turn"), ["dot", "Your turn"]);
  assert.deepEqual(status("your_turn", true), ["filled", "Your turn, not seen"]);
  assert.deepEqual(status("idle"), ["ring", "Idle"]);
  // An unseen turn keeps its mark once the session goes idle (danielrispler/Falinks#556).
  assert.deepEqual(status("idle", true), ["filled", "Idle, not seen"]);
  assert.deepEqual(status("ended"), ["ring", "Ended"]);
  // A subagent's own status, such as "completed", is its word.
  assert.deepEqual(status("completed"), ["ring", "completed"]);
});

test("only needs you uses colour in the list, the strip and the top bar", () => {
  const css = fs.readFileSync(new URL("../style.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const rules = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)].map(([, selector, body]) => [selector.trim(), body]);
  const ours = rules.filter(([selector]) => /\.row|\.strip|\.glyph|\.chip|\.top/.test(selector));
  assert.ok(ours.length > 5);
  const coloured = ours.filter(([, body]) => /--needs-you/.test(body)).map(([selector]) => selector);
  assert.ok(coloured.length > 0);
  assert.deepEqual(coloured.filter((selector) => !/pill|needs_you|\.chip/.test(selector)), []);
});

test("only running and needs-you rows get a second line: the step, or the question's first line", () => {
  const asked = { tool_use_id: "t", agent: null, answer: "open", questions: [{ question: "Which name?\nMore detail", options: [] }] };
  const done = { ...asked, answer: { answered: {} }, questions: [{ question: "Old", options: [] }] };
  assert.equal(secondLine({ state: "running", line: "Edit src/lib.rs", questions: [] }), "Edit src/lib.rs");
  assert.equal(secondLine({ state: "needs_you", line: "Bash ls · 3m", questions: [done, asked] }), "Which name?");
  // A plan or permission has no question: the row's line says who asks.
  assert.equal(secondLine({ state: "needs_you", line: "Bash ls · 3m", questions: [done] }), "Bash ls · 3m");
  // A redacted question with no text falls back too.
  assert.equal(secondLine({ state: "needs_you", line: "Bash ls · 3m", questions: [{ ...asked, questions: [{ options: [] }] }] }), "Bash ls · 3m");
  for (const state of ["your_turn", "idle", "ended"]) assert.equal(secondLine({ state, line: "Done.", questions: [] }), null);
});

const session = (id, name, folder = name, issue = null) => ({ id, name: null, folder_name: folder, identity: { issue, name } });

test("the strip's labels come from the names and are distinct when names share a prefix", () => {
  const labels = stripLabels([
    session("a", "Falinks"),
    session("b", "Falinks"),
    session("c", "Falinks-review"),
    session("d", "demo"),
    session("e", "#480 · PR #490", "agent-1", 480),
    session("f", "issue-322-terminals"),
  ]);
  assert.equal(new Set(labels).size, labels.length);
  assert.deepEqual(labels.slice(3), ["demo", "480", "322"]);
  assert.ok(labels.slice(0, 3).every((label) => label.startsWith("Fali")));
});

test("the chip counts needs you only, and hides at zero", () => {
  assert.equal(chipText([{ state: "needs_you" }, { state: "running" }, { state: "your_turn" }]), "1 needs you");
  assert.equal(chipText([{ state: "needs_you" }, { state: "needs_you" }]), "2 need you");
  assert.equal(chipText([{ state: "running" }]), "");
});
