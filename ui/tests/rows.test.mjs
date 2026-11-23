// One-line rows (danielrispler/Falinks#572): status by shape, a word with every glyph, colour only for needs you, distinct
// strip labels and the needs-you chip. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { chipText, clearing, CLEAR_WINDOW_MS, landedToClear, landedWord, secondLine, showsLanded, sidebarSessions, status, stripLabels } from "../../dist/list.js";

test("each state has its glyph and a word for its title and accessible name", () => {
  assert.deepEqual(status("needs_you"), ["pill", "Needs you"]);
  assert.deepEqual(status("running"), ["spinner", "Running"]);
  assert.deepEqual(status("your_turn"), ["dot", "Your turn"]);
  assert.deepEqual(status("your_turn", true), ["filled", "Your turn, not seen"]);
  assert.deepEqual(status("idle"), ["ring", "Idle"]);
  // An unseen turn keeps its mark once the session goes idle (danielrispler/Falinks#556).
  assert.deepEqual(status("idle", true), ["filled", "Idle, not seen"]);
  assert.deepEqual(status("ended"), ["ring", "Ended"]);
  // Idle with no process to watch is a guess, so it says so (#330); only idle does.
  assert.deepEqual(status("idle", false, false, true), ["ring", "Idle?"]);
  assert.deepEqual(status("idle", true, false, true), ["filled", "Idle?, not seen"]);
  assert.deepEqual(status("running", false, false, true), ["spinner", "Running"]);
  // A subagent's own status, such as "completed", is its word.
  assert.deepEqual(status("completed"), ["ring", "completed"]);
});

test("only needs you uses colour in the list, the strip and the top bar", () => {
  const css = fs.readFileSync(new URL("../style.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const rules = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)].map(([, selector, body]) => [selector.trim(), body]);
  const ours = rules.filter(([selector]) => /\.row|\.strip|\.glyph|\.chip|\.top|\.logo/.test(selector));
  assert.ok(ours.length > 5);
  const coloured = ours.filter(([, body]) => /--needs-you/.test(body)).map(([selector]) => selector);
  assert.ok(coloured.length > 0);
  assert.deepEqual(coloured.filter((selector) => !/pill|needs_you|\.chip/.test(selector)), []);
});

test("a row with blocked commands waiting reads how many, ahead of the step but not an open question", () => {
  const cards = (...statuses) => statuses.map((status) => ({ status }));
  const row = (state, blocked_commands) => ({ state, line: "Edit src/lib.rs", questions: [], blocked_commands });
  assert.equal(secondLine(row("running", cards("blocked"))), "1 blocked command waiting");
  assert.equal(secondLine(row("running", cards("blocked", "blocked_again", "allowed", "declined"))), "2 blocked commands waiting");
  assert.equal(secondLine(row("needs_you", cards("blocked"))), "1 blocked command waiting");
  // An allowed card has no button, so it does not count; one allowed but not run does.
  assert.equal(secondLine(row("your_turn", cards("allowed_not_run", "blocked_again", "running", "finished"))), "2 blocked commands waiting");
  assert.equal(secondLine(row("running", cards("allowed"))), "Edit src/lib.rs");
  assert.equal(secondLine(row("running", cards("declined"))), "Edit src/lib.rs");
  assert.equal(secondLine(row("ended", cards("blocked"))), null);
  const asked = { tool_use_id: "t", agent: null, answer: "open", questions: [{ question: "Which name?", options: [] }] };
  assert.equal(secondLine({ ...row("needs_you", cards("blocked")), questions: [asked] }), "Which name?");
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

test("a landed task's session at your turn, idle or ended shows a check mark and its word instead of its dot or ring", () => {
  assert.equal(landedWord, "Landed — safe to delete");
  for (const state of ["your_turn", "idle", "ended"]) {
    assert.deepEqual(status(state, false, true), ["check", "Landed — safe to delete"]);
    // Even unseen: the mark replaces the filled dot.
    assert.deepEqual(status(state, true, true), ["check", "Landed — safe to delete"]);
    assert.ok(showsLanded(state, true));
  }
  // Running and needs-you keep their own glyphs, and an unlanded row is as before.
  assert.deepEqual(status("running", false, true), ["spinner", "Running"]);
  assert.deepEqual(status("needs_you", false, true), ["pill", "Needs you"]);
  assert.deepEqual(status("idle", false, false), ["ring", "Idle"]);
  assert.ok(!showsLanded("idle", false));
});

// The list holds what Home held (#382): an unseen row shows its last Headline, a running row its Intention in place of the raw step,
// a needs-you row its question; no other row has a second line.
test("second lines by state: the question, the Intention, the last Headline, and none for the rest", () => {
  const headline = (s) => (s.turns.length ? `Headline of ${s.id}` : null);
  const row = (id, state, extra = {}) => ({ id, state, line: "Edit src/lib.rs", questions: [], unseen: false, landed: false, turns: [{}], transcript: { intention: null }, ...extra });
  const asked = { tool_use_id: "t", agent: null, answer: "open", questions: [{ question: "Which name?", options: [] }] };
  assert.equal(secondLine(row("n", "needs_you", { questions: [asked] }), headline), "Which name?");
  assert.equal(secondLine(row("r", "running", { transcript: { intention: "Make the rows reachable by keyboard." } }), headline), "Make the rows reachable by keyboard.");
  // Before its Intention is known, a running row shows its step.
  assert.equal(secondLine(row("r", "running"), headline), "Edit src/lib.rs");
  assert.equal(secondLine(row("u", "your_turn", { unseen: true }), headline), "Headline of u");
  assert.equal(secondLine(row("u", "idle", { unseen: true }), headline), "Headline of u");
  // An unseen row with no turn has nothing to say.
  assert.equal(secondLine(row("u", "your_turn", { unseen: true, turns: [] }), headline), null);
  // A landed row reads as landed, not unseen; seen, idle and ended rows have none.
  assert.equal(secondLine(row("l", "your_turn", { unseen: true, landed: true }), headline), null);
  for (const state of ["your_turn", "idle", "ended"]) assert.equal(secondLine(row("s", state), headline), null);
});

// Clear landed at the list's end (#382, #244): it hides the landed rows, Undo brings them back, and nothing is deleted inside the window.
test("Clear landed hides the landed rows, Undo restores them, and nothing is deleted inside the window", () => {
  const listed = [
    { id: "r", state: "running", landed: false },
    { id: "a", state: "idle", landed: true },
    { id: "b", state: "your_turn", landed: true },
    { id: "e", state: "ended", landed: true },
  ];
  const ids = (sessions) => sessions.map((s) => s.id);
  let due = null;
  const time = { set: (run) => (due = run), clear: () => (due = null) };
  const deleted = [];
  const m = clearing(time, { landed: (id) => listed.find((s) => s.id === id && s.landed), commit: (s) => void deleted.push(s.id), changed() {} });
  // Before Clear the list shows the landed rows, and offers Clear for every landed session, ended ones too.
  assert.deepEqual(ids(sidebarSessions(listed, m.hidden)), ["r", "a", "b"]);
  assert.deepEqual(ids(landedToClear(listed, m.hidden)), ["a", "b", "e"]);
  m.clear(landedToClear(listed, m.hidden));
  assert.deepEqual(ids(sidebarSessions(listed, m.hidden)), ["r"]);
  // With nothing landed left in view, Clear landed is not shown.
  assert.deepEqual(landedToClear(listed, m.hidden), []);
  assert.deepEqual(deleted, []);
  m.undo();
  assert.deepEqual(ids(sidebarSessions(listed, m.hidden)), ["r", "a", "b"]);
  assert.equal(due, null, "Undo stops the window");
  assert.deepEqual(deleted, []);
  assert.equal(CLEAR_WINDOW_MS, 8000);
});
