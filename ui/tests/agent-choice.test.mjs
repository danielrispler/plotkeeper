// The picker's agent choice: New session starts the agent Daniel last chose, New terminal a plain
// terminal, which is never kept as his choice. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { agentChoice, kept, showsAuto } from "../../dist/agents.js";
import { rowMark } from "../../dist/list.js";

test("the picker offers the installed agents, then a terminal", () => {
  assert.deepEqual(agentChoice(["claude", "agy", "codex"], "agy", "session"), { offered: ["claude", "agy", "codex", "terminal"], chosen: "agy" });
  assert.deepEqual(agentChoice(["claude", "agy", "codex"], "agy", "terminal").chosen, "terminal");
});

test("a last choice that is no longer installed falls back to Claude Code, else the first installed", () => {
  assert.equal(agentChoice(["claude", "agy"], "codex", "session").chosen, "claude");
  assert.equal(agentChoice(["claude"], null, "session").chosen, "claude");
  assert.equal(agentChoice(["agy", "codex"], null, "session").chosen, "agy");
});

test("a terminal is never kept as the choice; an agent is", () => {
  assert.equal(kept("terminal", "agy"), "agy");
  assert.equal(kept("codex", "agy"), "codex");
});

test("Auto mode shows only for Claude Code", () => {
  assert.deepEqual(["claude", "agy", "codex", "terminal"].map(showsAuto), [true, false, false, false]);
});

test("a plain terminal's row is marked as a terminal", () => {
  const row = { status_only: true, shell: true, transcript: { context: null }, capabilities: { agent: "" } };
  assert.equal(rowMark(row), "terminal");
});
